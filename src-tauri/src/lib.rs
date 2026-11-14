//! Точка входа Tauri-приложения SplitRoute.

pub mod catalog;
pub mod commands;
pub mod config;
pub mod logging;
pub mod privileged;
pub mod store;
pub mod subscription;
pub mod traffic;
pub mod tray;

use std::sync::Arc;

use privileged::{HelperClient, HelperInstaller};
use tauri::Manager;

/// Dev-режим без helper'а и TUN: `SPLITROUTE_MOCK_HELPER=1` в debug-сборке.
fn mock_helper_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var("SPLITROUTE_MOCK_HELPER")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true"))
}

#[cfg(unix)]
fn default_helper() -> Arc<dyn HelperClient> {
    if mock_helper_enabled() {
        return Arc::new(privileged::MockHelperClient::new());
    }
    Arc::new(privileged::UnixHelperClient::from_env())
}

#[cfg(windows)]
fn default_helper() -> Arc<dyn HelperClient> {
    if mock_helper_enabled() {
        return Arc::new(privileged::MockHelperClient::new());
    }
    Arc::new(privileged::WindowsPipeClient::from_env())
}

// Прочие цели (мобильные): helper'а нет, работает mock.
#[cfg(not(any(unix, windows)))]
fn default_helper() -> Arc<dyn HelperClient> {
    Arc::new(privileged::MockHelperClient::new())
}

fn default_installer() -> Arc<dyn HelperInstaller> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(privileged::MacosInstaller::new())
    }
    #[cfg(target_os = "linux")]
    {
        Arc::new(privileged::LinuxInstaller::new())
    }
    #[cfg(target_os = "windows")]
    {
        Arc::new(privileged::WindowsInstaller::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Arc::new(privileged::UnsupportedInstaller)
    }
}

/// rustls без `aws-lc`: провайдер ring регистрируется один раз до первого запроса.
pub(crate) fn init_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tls();

    let state = commands::AppState::new(
        default_helper(),
        default_installer(),
        store::Store::default_location(),
        Arc::new(subscription::ReqwestFetcher::new()),
    );
    if mock_helper_enabled() {
        state.logs.push(
            splitroute_protocol::LogLevel::Info,
            "Dev-режим: MockHelperClient, реальный туннель не поднимается",
        );
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .manage(state)
        .setup(|app| {
            tray::create(app.handle())?;
            let state = app.state::<commands::AppState>();
            if state.store.settings().preferences.launch_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<commands::AppState>();
                if state.store.settings().preferences.close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::helper_status,
            commands::connect,
            commands::disconnect,
            commands::helper_install,
            commands::helper_uninstall,
            commands::subscription_fetch,
            commands::profiles_list,
            commands::profile_select,
            commands::rules_list,
            commands::rules_upsert,
            commands::rules_delete,
            commands::apps_installed,
            commands::app_rule_from_installed,
            commands::app_icon,
            commands::settings_get,
            commands::settings_update,
            commands::hwid_generate,
            commands::logs_list,
            commands::diagnostics_get,
            commands::traffic_stats,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска SplitRoute");
}
