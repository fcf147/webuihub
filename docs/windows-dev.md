# Windows 开发构建指南（WebUI Hub）

本文档面向在 **Windows 11 + WSL2** 开发机上继续开发、构建 WebUI Hub 的开发者。
骨架代码已在 Linux 构建机产出但**未编译验证**，所有 Rust 后端逻辑需在本机真正跑通。

---

## 0. 前置概念

- 桌面壳本体是 **Tauri 2.x 应用**：Rust 负责系统能力（WSL 管理、HTTP 代理、健康检查），
  React 前端通过 Tauri 的 `invoke` 调用 Rust 暴露的命令。
- **WSL 后端第一步只做 DeepSeek Harness**：壳在 Windows 侧检测/安装 WSL，于默认发行版内
  安装并启动 `dsh web`，其端口经 WSL2 转发到 `127.0.0.1` 后由壳内 `<webview>` 加载。
- 开发机必须是 Windows（Tauri 编译 Windows 包需要 Windows + WebView2 + MSVC 工具链）。
  纯 Linux/macOS 无法产出 Windows 安装包。

---

## 1. 环境准备

### 1.1 系统与 WSL

```powershell
# 以管理员身份打开 PowerShell
wsl --install            # 安装 WSL2 + 默认 Ubuntu，完成后需重启
wsl --list --verbose     # 确认有默认发行版且 STATE=Running
```
> 若只是启用、没有发行版：`wsl --install -d Ubuntu`。
> 休眠/网络异常后可 `wsl --shutdown` 重置网络栈（端口转发断连时常用）。

### 1.2 Node.js（前端）

- 安装 **Node.js 20 LTS**（或 ≥18 的长期支持版）。
- 用官方安装包或 `nvm-windows` 均可，建议开启 corepack 或直接用 npm。

```powershell
node -v   # 例如 v20.18.0
npm -v
```

### 1.3 Rust 工具链（后端）

```powershell
# 从 https://rustup.rs 安装，选默认（stable）
rustup default stable
rustc -V
cargo -V
```

### 1.4 Tauri 系统依赖（Windows）

1. **Microsoft C++ Build Tools**：安装「使用 C++ 的桌面开发」工作负载（含 MSVC + Windows 10/11 SDK）。
   直接下载：https://visualstudio.microsoft.com/zh-hans/visual-cpp-build-tools/
2. **WebView2**：Windows 11 自带；Win10 从 https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/
   安装「常青版运行时」（Evergreen Bootstrapper）。
3. **git**：确保 `git` 在 PATH。

### 1.5 克隆与安装

```powershell
git clone https://github.com/fcf147/deepseek-harness-desktop.git
cd deepseek-harness-desktop
npm install
```

---

## 2. 补齐图标（必须，否则 build/dev 失败）

`src-tauri/icons/` 当前只有说明文件，**缺少真实图标**。`tauri.conf.json` 的
`bundle.icon` 与 `trayIcon.iconPath` 都引用了 `icons/icon.png` 等。

准备一张 **1024×1024 的 PNG** 源图（如 `assets/icon-1024.png`），然后：

```powershell
npm run tauri icon assets/icon-1024.png
```

该命令会生成：
```
src-tauri/icons/
├── 32x32.png
├── 128x128.png
├── 128x128@2x.png
├── icon.icns      # mac 用，Windows 打包也会引用但可忽略
├── icon.ico
└── icon.png
```
生成后删除 `src-tauri/icons/README.md`（如果存在）。
> 注意：tray 图标 `tauri.conf.json` 的 `trayIcon.iconPath` 是 `icons/icon.png`，
> 而 `bundle.icon` 列表里**没有** `icon.png`。若报缺 `icon.png`，把它加入 `bundle.icon`
> 或改 `trayIcon.iconPath` 指向已生成的某个文件。

---

## 3. 开发模式（最快验证）

```powershell
npm run tauri dev
```

- 会先执行 `beforeDevCommand`（`npm run dev`，即 Vite 在 `http://localhost:1420` 起前端热更新），
  再启动 Rust 后端并打开窗口。
- 第一次编译 Rust 会拉取依赖并编译（几分钟）。
- 窗口加载 React 壳；左侧应出现「WSL 后端」状态与「DeepSeek Harness」服务。

### 3.1 验证 WSL 后端

1. 左侧 WSL 状态应为 `ready`（否则按提示安装/启用 WSL）。
2. 选中 `DeepSeek Harness` → 点「一键安装」：
   触发 `install_service` → Rust 在 WSL 默认发行版内执行
   `curl ... bootstrap-dsh.sh | bash`（脚本里锁 Node 20 + dsh 版本）。
   日志面板会流式显示安装输出。
3. 安装完成后状态变 `installed` → 点「启动」：
   Rust 在 WSL 内 `spawn` `dsh web --port 3080`，然后用 `127.0.0.1:3080`
   做带重试的健康检查（12 次 / 2.5s）。通过后状态 `running`，右侧 WebView 加载 dsh UI。

> **重要**：`install_service` 与 `start_service` 当前通过 `wsl -d <distro> -- bash -lc ...`
> 调用。请确保默认发行版内已装好 `curl`、`bash`。Ubuntu 默认具备；最小镜像需先
> `apt install curl`。

---

## 4. 生产打包

```powershell
npm run tauri build
```

- 先 `beforeBuildCommand`（`npm run build`：tsc + vite build 产物到 `dist/`），
  再用 Rust 编译 release 并生成安装包。
- 产物在 `src-tauri/target/release/bundle/`：
  - `msi/WebUI Hub_<version>_x64_en-US.msi`
  - `nsis/WebUI Hub_<version>_x64-setup.exe`

### 4.1 关于签名（可选）

默认产物**未签名**。未签名安装包在 Windows 上会弹 SmartScreen 警告。
如需消除，配置代码签名证书并在 `tauri.conf.json` 的 `bundle.windows` 里设置
`certificateThumbprint` / 通过 `TAURI_SIGNING_*` 环境变量传入。初期可忽略。

---

## 5. 目录与开发入口速查

```
./
├── config/services.yaml        # 改这里增删服务 / 改端口 / 改版本锁
├── scripts/bootstrap-dsh.sh    # 改这里调整 WSL 内 Node/dsh 安装逻辑
├── src/                        # 前端（改 UI / API 封装）
│   ├── api/wsl.ts              # Tauri invoke 封装，命令名必须和 Rust 端一致
│   ├── api/proxy.ts            # api_proxy 模式服务用
│   ├── config/services.ts      # services.yaml 解析
│   ├── components/  pages/     # 界面
├── src-tauri/src/              # Rust 后端（改系统能力逻辑）
│   ├── wsl.rs                  # WSL 检测/安装/进程管理
│   ├── health.rs               # 健康检查（重试策略）
│   ├── proxy.rs                # HTTP 反向代理
│   ├── commands.rs             # #[tauri::command]，前端 invoke 的落点
│   ├── lib.rs                  # 注册命令 + 托管状态
```

**新增一个 Tauri 命令的标准流程**：
1. 在 `src-tauri/src/commands.rs` 写 `#[tauri::command] fn xxx(...) -> Result<_, String>`；
2. 在 `lib.rs` 的 `generate_handler!([...])` 里加入 `commands::xxx`；
3. 前端 `src/api/*.ts` 用 `invoke('xxx', {...})` 调用，注意参数名/类型一一对应。

---

## 6. 已知坑与排查

| 现象 | 原因 | 处理 |
|------|------|------|
| `tauri dev` 报找不到 WebView2 | 未装运行时 | 装 Evergreen WebView2 运行时 |
| 编译报 `link.exe` 失败 | 缺 MSVC / Windows SDK | 装 C++ 桌面开发工作负载 |
| 安装服务时 `curl: command not found` | 最小发行版无 curl | WSL 内 `apt install curl` |
| 启动后 WebView 空白 / 端口连不上 | WSL 端口转发断连（休眠后常见） | 日志看健康检查失败；`wsl --shutdown` 后重试 |
| 浏览器能用 `localhost:3080` 但壳用 `127.0.0.1` 不行（或反过来） | 部分 VPN/代理劫持 localhost 解析 | 已在 services.yaml 统一用 `127.0.0.1`；若仍有问题，检查 VPN 的 localhost 代理规则 |
| `dsh` 安装后版本与壳预期不符导致启动参数变化 | 上游 CLI 变更 | bootstrap 脚本锁了 `DSH_VERSION`，更新该变量即可 |
| 图标缺失导致 build 失败 | icons 未生成 | 见第 2 节 `tauri icon` |
| `wsl --install` 后仍未就绪 | 需重启系统 | 重启后 `wsl --list -v` 确认，必要时 `wsl --install -d Ubuntu` |

---

## 7. 调试技巧

- **Rust 端日志**：在 `commands.rs` 用 `println!` 或 `tauri::api::...`，dev 窗口的控制台也会打印。
- **前端日志**：React 壳的日志面板已接 `appendLog`，安装/启动过程流式可见。
- **单独验证 WSL 命令**：在 PowerShell 直接跑
  `wsl -d Ubuntu -- bash -lc "dsh web --port 3080"`，确认 dsh 在 WSL 内可启动、
  监听 `127.0.0.1:3080`，且在 Windows 侧 `curl http://127.0.0.1:3080/` 可达。
- **仅改前端**：`npm run dev` 直接起 Vite（不含 Rust），改 UI 快速迭代；涉及 invoke 时仍需 `tauri dev`。

---

## 8. 提交与同步

开发完成后在 `webui-shell` 分支提交并推送到 GitHub：

```powershell
git checkout webui-shell
git add -A
git commit -m "feat(desktop): ..."
git push -u origin webui-shell
```

> 注意：不要提交 `node_modules/`、`src-tauri/target/`、`dist/`（已在 `.gitignore`）。
> 真实图标（`src-tauri/icons/*.png` 等）应提交，但 `icons/README.md` 占位说明应删除。
