//! WSL 管理器（Rust 后端）
//! ----------------------------------------------------------------------------
//! 负责：
//!   - 检测 WSL 是否可用（`wsl --status` / `wsl -l -v`）
//!   - 未安装时触发 `wsl --install`（需管理员，返回需重启）
//!   - 枚举发行版、找出默认发行版
//!   - 在指定发行版内执行命令（安装 dsh / 启动 dsh web）
//!   - 管理服务子进程（启/停）
//!
//! 坑点规避（README 已知限制）：
//!   - WSL 安装需重启：install 返回 needs_reboot，由前端提示
//!   - 端口转发断连：health 模块负责重试（见 health.rs）
//!   - 非 Windows 平台（如 Linux 构建机）返回 NotSupported，无法实测

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Distro {
    pub name: String,
    pub state: String,
    pub version: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WslState {
    pub status: String,
    pub platform: String,
    pub distros: Vec<Distro>,
    pub default_distro: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub triggered: bool,
    pub needs_reboot: bool,
    pub message: String,
}

const STATUS_NOT_SUPPORTED: &str = "not_supported";
const STATUS_NOT_INSTALLED: &str = "not_installed";
const STATUS_NO_DISTRO: &str = "no_distro";
const STATUS_READY: &str = "ready";
const STATUS_NEEDS_REBOOT: &str = "needs_reboot";

fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

/// 解码 WSL 命令输出为干净 UTF-8 字符串。
///
/// WSL 在 Windows 上可能以 UTF-16LE 输出（每个 ASCII 字符后带 \x00），
/// 若直接用 from_utf8_lossy 解码会残留 NUL 字节，进而导致：
///   - 发行版列表解析出 "NAME"/"Unknown" 等脏数据（Bug 1）
///   - 把含 NUL 的发行版名传给 Command 时触发 "nul byte found in provided data"（Bug 2）
/// 这里做 UTF-16 探测并按 UTF-16LE 解码，最后再清除所有残留 NUL。
fn decode_wsl_output(bytes: &[u8]) -> String {
    // UTF-16LE 探测：统计 NUL 字节密度。纯 ASCII 的 UTF-8 应无 NUL；
    // UTF-16LE 的 ASCII 文本 NUL 占比接近一半。
    if !bytes.is_empty() {
        let nuls = bytes.iter().filter(|&&b| b == 0).count();
        if nuls > 0 && nuls * 2 >= bytes.len() {
            // 按 UTF-16LE 解码
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            if !units.is_empty() {
                if let Ok(s) = String::from_utf16(&units) {
                    return s.replace('\0', "");
                }
            }
        }
    }
    // 常规 UTF-8，清除任何残留 NUL
    String::from_utf8_lossy(bytes).replace('\0', "")
}

/// Windows 上禁止创建新的控制台窗口（避免每次命令都弹一个黑窗口）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 构造 wsl 命令并强制其以 UTF-8 输出（避免 UTF-16/GBK 乱码）。
/// WSL_UTF8=1 是微软官方支持的环境变量，让 wsl.exe 输出 UTF-8。
/// 在 Windows 上额外设置 CREATE_NO_WINDOW，防止每次调用弹出新的终端窗口。
fn wsl_command(args: &[&str]) -> Command {
    let mut cmd = Command::new("wsl");
    cmd.args(args);
    // 强制 wsl.exe 以 UTF-8 输出，消除本地化/编码导致的乱码
    cmd.env("WSL_UTF8", "1");
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 在 Windows 上运行 `wsl <args>`，返回 (code, stdout, stderr)。
fn run_wsl(args: &[&str]) -> (i32, String, String) {
    if !is_windows() {
        return (-1, String::new(), "WSL is only supported on Windows".into());
    }
    let output = wsl_command(args).output();
    match output {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            decode_wsl_output(&o.stdout),
            decode_wsl_output(&o.stderr),
        ),
        Err(e) => (-1, String::new(), e.to_string()),
    }
}

/// 在指定发行版内同步执行一条命令，返回 (code, stdout, stderr)。
pub fn exec_in_distro(distro: &str, command: &str) -> (i32, String, String) {
    if !is_windows() {
        return (-1, String::new(), "WSL is only supported on Windows".into());
    }
    let distro = distro.replace('\0', "");
    let output = wsl_command(&["-d", &distro, "--", "bash", "-lc", command]).output();
    match output {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            decode_wsl_output(&o.stdout),
            decode_wsl_output(&o.stderr),
        ),
        Err(e) => (-1, String::new(), e.to_string()),
    }
}

/// 检测指定发行版内 dsh 是否已安装。
pub fn is_dsh_installed(distro: &str) -> bool {
    let (code, out, _err) = exec_in_distro(distro, "command -v dsh >/dev/null 2>&1 && echo 1 || echo 0");
    if code != 0 {
        return false;
    }
    out.trim() == "1"
}

/// 检测指定发行版内 Open WebUI 是否已安装（检查 venv 内可执行）。
pub fn is_open_webui_installed(distro: &str) -> bool {
    let probe = "test -x \"$HOME/.venv/open-webui/bin/open-webui\" && echo 1 || echo 0";
    let (code, out, _err) = exec_in_distro(distro, probe);
    if code != 0 {
        return false;
    }
    out.trim() == "1"
}

/// 检测指定发行版内 Hermes Agent 是否已安装（检查 venv 内可执行）。
/// FHS root 安装：/usr/local/lib/hermes-agent/venv/bin/hermes
/// 普通用户安装：$HOME/.hermes/hermes-agent/venv/bin/hermes
pub fn is_hermes_agent_installed(distro: &str) -> bool {
    let probe = r#"test -x /usr/local/lib/hermes-agent/venv/bin/hermes \
-o -x "$HOME/.hermes/hermes-agent/venv/bin/hermes" && echo 1 || echo 0"#;
    let (code, out, _err) = exec_in_distro(distro, probe);
    if code != 0 {
        return false;
    }
    out.trim() == "1"
}

/// 检测 WSL 整体状态。
pub fn detect() -> WslState {
    let platform = if is_windows() { "win32" } else { "other" };
    if !is_windows() {
        return WslState {
            status: STATUS_NOT_SUPPORTED.into(),
            platform: platform.into(),
            distros: vec![],
            default_distro: None,
            hint: Some("WSL 仅支持 Windows 平台；当前为其他平台，无法运行 WSL 后端。".into()),
        };
    }

    let status_res = run_wsl(&["--status"]);
    if status_res.0 != 0 {
        // 可能是未安装。再用 --list 进一步判断。
        let list_res = run_wsl(&["--list", "--quiet"]);
        if list_res.0 != 0 {
            return WslState {
                status: STATUS_NOT_INSTALLED.into(),
                platform: platform.into(),
                distros: vec![],
                default_distro: None,
                hint: Some("WSL 未安装或被禁用。请以管理员身份运行 \"wsl --install\"。".into()),
            };
        }
    }

    let verbose = run_wsl(&["--list", "--verbose"]);
    let (distros, default_distro) = parse_distro_list(&verbose.1);

    if distros.is_empty() {
        return WslState {
            status: STATUS_NO_DISTRO.into(),
            platform: platform.into(),
            distros: vec![],
            default_distro: None,
            hint: Some("WSL 已启用但未安装发行版。可运行 \"wsl --install -d Ubuntu\" 安装。".into()),
        };
    }

    WslState {
        status: STATUS_READY.into(),
        platform: platform.into(),
        distros,
        default_distro,
        hint: None,
    }
}

/// 解析 `wsl --list --verbose` 输出，`*` 标记默认发行版。
fn parse_distro_list(text: &str) -> (Vec<Distro>, Option<String>) {
    let mut distros = Vec::new();
    let mut default_distro = None;
    for raw in text.lines() {
        let was_default = raw.trim_start().starts_with('*');
        let cleaned = raw.trim_start_matches('*').trim();
        if cleaned.is_empty() {
            continue;
        }
        let parts: Vec<&str> = cleaned.split_whitespace().collect();
        if parts.is_empty() || parts[0].chars().all(|c| c == '-') {
            continue;
        }
        // 跳过表头：用 split_whitespace 归一化后判断（原始串可能含多空格）
        if parts[0].eq_ignore_ascii_case("NAME") {
            continue;
        }
        let name = parts[0].to_string();
        // 过滤含 NUL 的畸形名称（异常编码残留）
        if name.contains('\0') || name.is_empty() {
            continue;
        }
        let state = parts.get(1).unwrap_or(&"Unknown").to_string();
        let version = parts.get(2).unwrap_or(&"").to_string();
        if was_default {
            default_distro = Some(name.clone());
        }
        distros.push(Distro {
            name,
            state,
            version,
            is_default: was_default,
        });
    }
    (distros, default_distro)
}

/// 触发 WSL 安装。需管理员权限；成功后通常需重启。
pub fn install() -> InstallResult {
    if !is_windows() {
        return InstallResult {
            triggered: false,
            needs_reboot: false,
            message: "非 Windows 平台，无法安装 WSL。".into(),
        };
    }
    let (code, _out, err) = run_wsl(&["--install"]);
    if code == 0 {
        InstallResult {
            triggered: true,
            needs_reboot: true,
            message: "已触发 WSL 安装，请重启系统后重试。".into(),
        }
    } else {
        InstallResult {
            triggered: false,
            needs_reboot: false,
            message: format!("WSL 安装失败：{}（可能需要以管理员身份运行）", err),
        }
    }
}

/// 在指定发行版内执行 shell 命令。返回子进程句柄（用于停止）。
/// 这里以 spawn 方式保留句柄，使调用方能 kill。
pub fn spawn_in_distro(distro: &str, command: &str) -> Option<Child> {
    if !is_windows() {
        return None;
    }
    wsl_command(&["-d", distro, "--", "bash", "-lc", command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()
}

/// 在指定发行版内通过 stdin 把脚本内容喂给 `bash` 执行。
///
/// 相比 `bash -s`，这里把脚本写入 WSL 临时文件后用 `/bin/bash` 显式执行，
/// 避免执行 shell 被解析为 sh/dash 而报 "set: pipefail: invalid option name"。
///
/// `on_line(stream, line)` 会在每行输出时被回调（stream 为 "out" 或 "err"），
/// 用于前端流式显示安装日志；返回值仍包含完整 stdout / stderr 供后续使用。
pub fn exec_in_distro_stdin(
    distro: &str,
    script: &str,
    on_line: impl Fn(&str, &str) + Send + Sync + 'static,
) -> (i32, String, String) {
    if !is_windows() {
        return (-1, String::new(), "WSL is only supported on Windows".into());
    }
    // 防御：清理发行版名中的 NUL（避免 Command spawn 时报 nul byte found）
    let distro = distro.replace('\0', "");

    // 命令：cat > /tmp/webui_bootstrap.sh && /bin/bash /tmp/webui_bootstrap.sh
    let mut child = match wsl_command(&[
        "-d",
        &distro,
        "--",
        "/bin/bash",
        "-c",
        "cat > /tmp/webui_bootstrap.sh && /bin/bash /tmp/webui_bootstrap.sh",
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    {
        Ok(c) => c,
        Err(e) => return (-1, String::new(), e.to_string()),
    };

    // 写入脚本到 stdin 并关闭，触发 cat 写文件 + bash 执行。
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        // 兜底：移除 CRLF 的 \r（若源码被 git 转成 CRLF，bash 会因 \r 报语法错误）
        let cleaned = script.replace("\r\n", "\n");
        if let Err(e) = stdin.write_all(cleaned.as_bytes()) {
            return (-1, String::new(), format!("写入脚本失败: {}", e));
        }
        // stdin 在此 drop，关闭管道。
    }

    // 用 Arc 共享 on_line 回调，供两个读取线程调用。
    let on_line_arc: std::sync::Arc<dyn Fn(&str, &str) + Send + Sync> =
        std::sync::Arc::new(on_line);

    let stdout_full: std::sync::Arc<std::sync::Mutex<String>> =
        std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let stderr_full: std::sync::Arc<std::sync::Mutex<String>> =
        std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    // 逐行读取一个管道：实时回调 + 累积完整输出到 shared buffer。
    fn pump<R: std::io::Read + Send + 'static>(
        reader: R,
        stream: &'static str,
        cb: std::sync::Arc<dyn Fn(&str, &str) + Send + Sync>,
        buf: std::sync::Arc<std::sync::Mutex<String>>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(reader).lines() {
                let l = match line {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                cb(stream, &l);
                if let Ok(mut s) = buf.lock() {
                    s.push_str(&l);
                    s.push('\n');
                }
            }
        })
    }

    let handle_out = child
        .stdout
        .take()
        .map(|so| pump(so, "out", on_line_arc.clone(), stdout_full.clone()));
    let handle_err = child
        .stderr
        .take()
        .map(|se| pump(se, "err", on_line_arc.clone(), stderr_full.clone()));

    let status = child.wait();
    if let Some(h) = handle_out {
        let _ = h.join();
    }
    if let Some(h) = handle_err {
        let _ = h.join();
    }

    let code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
    let out = stdout_full
        .lock()
        .map(|g| decode_wsl_output(g.as_bytes()))
        .unwrap_or_default();
    let err = stderr_full
        .lock()
        .map(|g| decode_wsl_output(g.as_bytes()))
        .unwrap_or_default();
    (code, out, err)
}

/// 进程注册表：service_id -> child。
/// 使用 Mutex 保护；停止时 kill 对应进程。
pub struct ProcessRegistry {
    inner: std::sync::Mutex<HashMap<String, Child>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, id: &str, child: Child) {
        let mut g = self.inner.lock().unwrap();
        g.insert(id.to_string(), child);
    }

    pub fn stop(&self, id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        if let Some(mut child) = g.remove(id) {
            let _ = child.kill();
            let _ = child.wait();
            true
        } else {
            false
        }
    }

    pub fn has(&self, id: &str) -> bool {
        let g = self.inner.lock().unwrap();
        g.contains_key(id)
    }
}
