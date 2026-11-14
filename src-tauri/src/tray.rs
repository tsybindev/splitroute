//! Трей и системная интеграция: меню, статус, показ окна, выход.
//!
//! Иконка и статус отражают состояние ядра; действия вызывают то же ядро, что и
//! команды UI, чтобы не было двух источников истины (AGENTS.md §10).

use std::sync::Mutex;

use splitroute_protocol::{CoreState, HelperStatus};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::commands::{self, AppState};

/// Пункты меню, которые нужно обновлять при смене статуса.
#[derive(Default)]
pub struct TrayState {
    status_item: Mutex<Option<MenuItem<Wry>>>,
    toggle_item: Mutex<Option<MenuItem<Wry>>>,
}

pub const TRAY_ID: &str = "splitroute-tray";

/// Создаёт иконку и меню трея. Вызывается один раз в `setup`.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, "status", status_label(None), false, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, "toggle", "Подключить", true, None::<&str>)?;
    let open_item = MenuItem::with_id(app, "open", "Открыть SplitRoute", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status_item, &toggle_item, &open_item, &quit_item])?;

    // macOS ждёт template-иконку (альфа-канал, без цвета), остальные ОС —
    // цветную 32×32: белая моно-иконка на светлой теме была бы невидимой.
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("SplitRoute")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => toggle(app),
            "open" => show_main(app),
            "quit" => quit(app),
            _ => {}
        });
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    builder.build(app)?;

    app.manage(TrayState {
        status_item: Mutex::new(Some(status_item)),
        toggle_item: Mutex::new(Some(toggle_item)),
    });
    Ok(())
}

/// Обновляет меню и рассылает статус в UI.
pub fn publish(app: &AppHandle, status: &HelperStatus) {
    let label = status_label(Some(status.state));
    let toggle = if status.state == CoreState::Running {
        "Отключить"
    } else {
        "Подключить"
    };
    if let Some(state) = app.try_state::<TrayState>() {
        if let Ok(item) = state.status_item.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(label);
            }
        }
        if let Ok(item) = state.toggle_item.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(toggle);
            }
        }
    }
    use tauri::Emitter;
    let _ = app.emit("connection-changed", status);
}

fn status_label(state: Option<CoreState>) -> &'static str {
    match state {
        Some(CoreState::Running) => "Подключено",
        Some(CoreState::Stopped) | None => "Не подключено",
    }
}

fn toggle(app: &AppHandle) {
    let state = app.state::<AppState>();
    let running = state
        .helper
        .status()
        .map(|status| status.state == CoreState::Running)
        .unwrap_or(false);
    let result = if running {
        commands::disconnect_inner(&state)
    } else {
        commands::connect_inner(&state)
    };
    match result {
        Ok(status) => publish(app, &status),
        Err(error) => state
            .logs
            .push(splitroute_protocol::LogLevel::Error, error.message),
    }
}

/// Показывает и фокусирует главное окно (single instance, трей).
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _ = commands::disconnect_inner(&state);
    app.exit(0);
}
