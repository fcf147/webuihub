//! Tauri command 注册（Rust 后端）
//! ----------------------------------------------------------------------------
//! 把 WSL 管理器 / 健康检查 / 代理 暴露给前端（src/api/wsl.ts、proxy.ts）。
//! 共享状态通过 Tauri 的 `manage` 注入。

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::health;
use crate::proxy::{self, ProxyRequest};
use crate::wsl::{self, ProcessRegistry, WslState};

/// 前端可见的服务运行状态。
#[derive(Debug, Clone, Serialize)]
pub struct ServiceRuntime {
    pub id: String,
    pub status: String,
    pub url: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

/// 托管状态：进程注册表（停止服务时使用）。
pub struct AppState {
    pub registry: Arc<ProcessRegistry>,
}

#[tauri::command]
pub fn get_wsl_state() -> WslState {
    wsl::detect()
}

#[tauri::command]
pub fn install_wsl() -> wsl::InstallResult {
    wsl::install()
}

// 编译期自动生成的脚本注册表（build.rs 枚举 scripts/ 下所有 .sh 文件）。
// 新增脚本只需放到 scripts/ 目录下，无需修改 Rust 代码。
include!(concat!(env!("OUT_DIR"), "/generated_scripts.rs"));

// 编译期内嵌的默认服务配置（项目根 config/services.yaml）。
// 首次运行时会被写出到 exe 旁的 config/services.yaml，供用户自由修改/新增服务。
const DEFAULT_SERVICES_YAML: &str = include_str!("../../config/services.yaml");

/// 返回 exe 所在目录（外部可编辑的 config/、scripts/ 都放这里）。
fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

/// 确保 exe 旁存在可编辑的 config/ 与 scripts/ 目录及默认文件。
/// 文件不存在则写入内置默认内容；已存在则保留用户修改，不做覆盖。
pub fn ensure_runtime_files() {
    let Some(dir) = exe_dir() else { return };
    // config/services.yaml
    let config_dir = dir.join("config");
    if std::fs::create_dir_all(&config_dir).is_ok() {
        let config_path = config_dir.join("services.yaml");
        if !config_path.exists() {
            let _ = std::fs::write(&config_path, DEFAULT_SERVICES_YAML);
        }
    }
    // scripts/bootstrap-*.sh（自动释放所有内置脚本）
    let scripts_dir = dir.join("scripts");
    if std::fs::create_dir_all(&scripts_dir).is_ok() {
        for (name, content) in all_scripts() {
            let script_path = scripts_dir.join(name);
            if !script_path.exists() {
                let _ = std::fs::write(&script_path, content);
            }
        }
    }
}

/// 读取 exe 旁的外部 bootstrap 脚本；若不存在则回退内置默认。
/// 优先使用 script_url 指定的文件名，兼容旧版按 id 猜测的命名。
fn load_bootstrap_script(id: &str, script_url: Option<&str>) -> String {
    let Some(dir) = exe_dir() else {
        return builtin_fallback(id).to_string();
    };
    // 从 script_url 提取文件名（如 "scripts/bootstrap-dsh.sh" → "bootstrap-dsh.sh"）
    let fname = script_url
        .and_then(|url| url.rsplit('/').next())
        .unwrap_or_else(|| {
            // 兼容旧版无 script_url 时按 id 猜测命名
            let fname = format!("bootstrap-{}.sh", id.replace('_', "-"));
            Box::leak(fname.into_boxed_str())
        });
    let path = dir.join("scripts").join(fname);
    std::fs::read_to_string(&path).unwrap_or_else(|_| builtin_fallback(id).to_string())
}

/// 回退到编译期内置的脚本内容。
fn builtin_fallback(id: &str) -> &'static str {
    // 先尝试精确匹配：bootstrap-<service_id>.sh
    for (name, _) in all_scripts() {
        let script_id = name
            .strip_prefix("bootstrap-")
            .and_then(|s| s.strip_suffix(".sh"))
            .unwrap_or("");
        if script_id == id {
            return get_script(name).unwrap_or("");
        }
    }
    // 再尝试 bootstrap-<id_with_underscores>.sh（兼容旧版命名）
    let fname = format!("bootstrap-{}.sh", id.replace('-', "_"));
    if let Some(content) = get_script(&fname) {
        return content;
    }
    // 最后回退到 bootstrap-dsh.sh
    get_script("bootstrap-dsh.sh").unwrap_or("")
}

/// 安装日志事件负载：stream 为 "info" | "out" | "error"，text 为日志文本。
#[derive(Clone, Serialize)]
pub struct InstallLog {
    pub stream: String,
    pub text: String,
}

/// 检测指定服务是否已安装（按 id 区分检测方式）。
#[tauri::command]
pub fn check_service_installed(id: String, distro: Option<String>) -> bool {
    let state = wsl::detect();
    let distro = distro.or(state.default_distro);
    match distro {
        Some(d) => match id.as_str() {
            "deepseek_harness" => wsl::is_dsh_installed(&d),
            "open_webui" => wsl::is_open_webui_installed(&d),
            "hermes_agent" => wsl::is_hermes_agent_installed(&d),
            _ => false,
        },
        None => false,
    }
}

/// 读取 exe 旁 config/services.yaml 的内容。
/// 供前端 loadServices 使用：外部文件存在则用它（用户可新增服务），
/// 不存在则返回内置默认内容。
#[tauri::command]
pub fn get_services_config() -> String {
    let Some(dir) = exe_dir() else {
        return DEFAULT_SERVICES_YAML.to_string();
    };
    let path = dir.join("config").join("services.yaml");
    std::fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_SERVICES_YAML.to_string())
}

/// 打开服务 UI：创建一个独立 WebviewWindow 加载目标 URL。
///
/// 之所以用独立窗口而非壳内 <webview> 标签：Tauri 2 的 <webview> 标签
/// 加载外部 http URL 在部分环境不可靠（黑屏），而 WebviewWindow 是
/// 官方稳定可靠的方式。若同名窗口已存在则复用并聚焦，避免重复创建。
#[tauri::command]
pub fn open_service_ui(app: AppHandle, label: String, url: String) -> Result<(), String> {
    // 若同名窗口已存在，直接聚焦并返回
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
        return Ok(());
    }
    let parsed = url.parse::<tauri::Url>().map_err(|e| format!("无效 URL: {}", e))?;
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
        .title("WebUI Hub")
        .inner_size(1200.0, 800.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn install_service(
    app: AppHandle,
    id: String,
    distro: Option<String>,
    script_url: Option<String>,
) -> Result<(), String> {
    let state = wsl::detect();
    if state.status != "ready" {
        return Err(state.hint.unwrap_or_else(|| "WSL 不可用".into()));
    }
    let distro = distro.or(state.default_distro).ok_or("无可用发行版")?;
    // 按服务 id 选择对应 bootstrap 脚本（优先读取 exe 旁可编辑脚本，否则用内置默认）。
    // 通过 stdin 喂给 WSL 内的 bash 执行，逐行把输出 emit 到前端做流式日志。
    let script = load_bootstrap_script(&id, script_url.as_deref());
    let app = app.clone();
    let (code, _out, err) = wsl::exec_in_distro_stdin(&distro, &script, move |stream, line| {
        // 校验 UTF-8，避免 emit 失败导致安装中断
        let _ = app.emit(
            "install-log",
            InstallLog {
                stream: if stream == "err" { "error".into() } else { "out".into() },
                text: line.to_string(),
            },
        );
    });
    if code != 0 {
        return Err(format!("安装失败: {}", err));
    }
    let _ = id;
    Ok(())
}

#[tauri::command]
pub async fn start_service(
    id: String,
    distro: Option<String>,
    autostart_cmd: String,
    health_url: String,
    state: State<'_, AppState>,
) -> Result<ServiceRuntime, String> {
    let wsl_state = wsl::detect();
    if wsl_state.status != "ready" {
        return Err(wsl_state.hint.unwrap_or_else(|| "WSL 不可用".into()));
    }
    let distro = distro.or(wsl_state.default_distro).ok_or("无可用发行版")?;

    // 在 WSL 内启动 dsh web；保留子进程句柄以便停止。
    let child = wsl::spawn_in_distro(&distro, &autostart_cmd).ok_or("无法在 WSL 内启动服务")?;
    state.registry.register(&id, child);

    // 健康检查（带重试，规避端口转发初始延迟/断连）
    let healthy = health::check_with_retry(&health_url, 12, 2500).await;
    if !healthy {
        state.registry.stop(&id);
        return Ok(ServiceRuntime {
            id,
            status: "error".into(),
            url: None,
            version: None,
            error: Some("健康检查失败，服务未在预期端口就绪".into()),
        });
    }

    Ok(ServiceRuntime {
        id,
        status: "running".into(),
        url: Some(health_url.clone()),
        version: None,
        error: None,
    })
}

#[tauri::command]
pub fn stop_service(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.registry.stop(&id))
}

#[tauri::command]
pub async fn health_check(url: String) -> Result<bool, String> {
    Ok(health::check_with_retry(&url, 3, 1000).await)
}

#[tauri::command]
pub async fn proxy_request(req: ProxyRequest) -> Result<String, String> {
    proxy::proxy(req).await
}
