# WebUI Hub

基于 **Tauri 2.x（Rust + 系统 WebView）** 的各种 WebUI 服务桌面集线器。本仓库即项目根。
第一步后端针对 **Windows + WSL2**，已实现 **DeepSeek Harness** 与 **Open WebUI** 两个服务：壳内置 WSL 生命周期管理，在默认发行版内安装并启动服务，端口经 WSL2 转发到 Windows `127.0.0.1` 后由壳内 WebView 加载。

**Windows 开发机构建步骤**见 [`docs/windows-dev.md`](./docs/windows-dev.md)。

## 目录结构

```
./
├── config/services.yaml         # 服务声明（第一步仅 deepseek_harness）
├── scripts/bootstrap-dsh.sh     # WSL 内 DSH 安装引导脚本
├── src/                         # 前端 (Vite + React + TS)
│   ├── components/              # Sidebar / WebViewPanel / LogPanel / StatusBadge
│   ├── pages/                   # ServiceList / ServiceDetail
│   ├── api/                     # proxy.ts / wsl.ts（Tauri invoke 封装）
│   ├── config/services.ts       # services.yaml 加载与解析
│   ├── App.tsx / main.tsx
│   └── styles.css
├── src-tauri/                   # Rust 后端
│   ├── src/                     # main.rs / lib.rs / wsl.rs / proxy.rs / health.rs / commands.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json / vite.config.ts / tsconfig.json
└── docs/windows-dev.md          # Windows 开发构建指南
```

## 已知坑（README 已知限制，骨架已规避/待处理）

- WSL 安装需重启：`wsl --install` 后壳提示重启。
- 端口转发断连：Windows 休眠/唤醒后 WSL 端口转发可能中断，`health.rs` 带重试；彻底恢复需 `wsl --shutdown`。
- VPN 劫持 localhost：services.yaml 用 `127.0.0.1` 而非 `localhost`。
- dsh 版本锁定：bootstrap 脚本与 services.yaml 锁版本，请按需更新。

## 待补齐（骨架阶段）

- `src-tauri/icons/` 图标文件（ico/png/icns）需补充，否则 `tauri build` 失败。
- Rust 后端需在 **Windows** 上真正编译验证（当前构建环境为 Linux，无 cargo，仅做骨架）。

## 构建与运行（需在 Windows + WSL2 开发机）

```bash
npm install
npm run tauri dev      # 开发模式
npm run tauri build    # 生产打包（msi / nsis）
```
