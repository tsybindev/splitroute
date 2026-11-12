//! Точка входа Tauri-приложения SplitRoute.

pub mod commands;
pub mod config;
pub mod privileged;
pub mod subscription;

use std::sync::Arc;

use privileged::{HelperClient, HelperInstaller, UnsupportedInstaller};

#[cfg(unix)]
fn default_helper() -> Arc<dyn HelperClient> {
    Arc::new(privileged::UnixHelperClient::from_env())
}

#[cfg(not(unix))]
fn default_helper() -> Arc<dyn HelperClient> {
    Arc::new(privileged::MockHelperClient::new())
}

fn default_installer() -> Arc<dyn HelperInstaller> {
    Arc::new(UnsupportedInstaller)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = commands::AppState::new(default_helper(), default_installer());
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::helper_status,
            commands::connect,
            commands::disconnect,
            commands::helper_install,
            commands::helper_uninstall,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска SplitRoute");
}
