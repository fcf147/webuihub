//! WebUI Hub — Tauri 应用入口（Rust 后端）

mod commands;
mod health;
mod proxy;
mod wsl;

use commands::AppState;
use wsl::ProcessRegistry;

/// 启动 Tauri 应用，注册命令并注入共享状态。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let registry = std::sync::Arc::new(ProcessRegistry::new());
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { registry })
        .invoke_handler(tauri::generate_handler![
            commands::get_wsl_state,
            commands::install_wsl,
            commands::check_service_installed,
            commands::install_service,
            commands::start_service,
            commands::open_service_ui,
            commands::stop_service,
            commands::health_check,
            commands::proxy_request,
            commands::get_services_config,
        ])
        .setup(|_app| {
            // 首次运行：确保 exe 旁生成可编辑的 config/services.yaml 与 scripts/ 下所有引导脚本
            commands::ensure_runtime_files();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running WebUI Hub");
}
