//! Трей и системная интеграция: меню, статус, показ окна, выход.
//!
//! Иконка и статус отражают состояние ядра; действия вызывают то же ядро, что и
//! команды UI, чтобы не было двух источников истины (AGENTS.md §10).

use std::sync::Mutex;

use splitroute_protocol::{CoreState, HelperStatus, Language};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::commands::{self, AppState};

/// Пункты меню, которые нужно обновлять при смене статуса.
#[derive(Default)]
pub struct TrayState {
    status_item: Mutex<Option<MenuItem<Wry>>>,
    toggle_item: Mutex<Option<MenuItem<Wry>>>,
    open_item: Mutex<Option<MenuItem<Wry>>>,
    quit_item: Mutex<Option<MenuItem<Wry>>>,
    /// Последнее известное состояние ядра: по нему пересобираются подписи при
    /// смене языка без запроса к helper'у.
    running: Mutex<bool>,
}

pub const TRAY_ID: &str = "splitroute-tray";

/// Создаёт иконку и меню трея. Вызывается один раз в `setup`.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let language = app
        .state::<AppState>()
        .store
        .settings()
        .preferences
        .language;
    let status_item = MenuItem::with_id(
        app,
        "status",
        status_label(None, language),
        false,
        None::<&str>,
    )?;
    let toggle_item = MenuItem::with_id(
        app,
        "toggle",
        toggle_label(false, language),
        true,
        None::<&str>,
    )?;
    let open_item = MenuItem::with_id(app, "open", open_label(language), true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", quit_label(language), true, None::<&str>)?;
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
        open_item: Mutex::new(Some(open_item)),
        quit_item: Mutex::new(Some(quit_item)),
        running: Mutex::new(false),
    });
    Ok(())
}

/// Обновляет меню и рассылает статус в UI.
pub fn publish(app: &AppHandle, status: &HelperStatus) {
    let language = app
        .state::<AppState>()
        .store
        .settings()
        .preferences
        .language;
    apply_labels(app, language, status.state == CoreState::Running);
    use tauri::Emitter;
    let _ = app.emit("connection-changed", status);
}

/// Перерисовывает подписи трея при смене языка интерфейса.
pub fn set_language(app: &AppHandle, language: Language) {
    let running = app
        .try_state::<TrayState>()
        .and_then(|state| state.running.lock().ok().map(|running| *running))
        .unwrap_or(false);
    apply_labels(app, language, running);
}

fn apply_labels(app: &AppHandle, language: Language, running: bool) {
    if let Some(state) = app.try_state::<TrayState>() {
        if let Ok(mut value) = state.running.lock() {
            *value = running;
        }
        let status = if running {
            Some(CoreState::Running)
        } else {
            Some(CoreState::Stopped)
        };
        if let Ok(item) = state.status_item.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(status_label(status, language));
            }
        }
        if let Ok(item) = state.toggle_item.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(toggle_label(running, language));
            }
        }
        if let Ok(item) = state.open_item.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(open_label(language));
            }
        }
        if let Ok(item) = state.quit_item.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(quit_label(language));
            }
        }
    }
}

fn status_label(state: Option<CoreState>, language: Language) -> &'static str {
    match (state, language) {
        (Some(CoreState::Running), Language::Ru) => "Подключено",
        (Some(CoreState::Running), Language::En) => "Connected",
        (_, Language::Ru) => "Не подключено",
        (_, Language::En) => "Disconnected",
    }
}

fn toggle_label(running: bool, language: Language) -> &'static str {
    match (running, language) {
        (true, Language::Ru) => "Отключить",
        (true, Language::En) => "Disconnect",
        (false, Language::Ru) => "Подключить",
        (false, Language::En) => "Connect",
    }
}

fn open_label(language: Language) -> &'static str {
    match language {
        Language::Ru => "Открыть SplitRoute",
        Language::En => "Open SplitRoute",
    }
}

fn quit_label(language: Language) -> &'static str {
    match language {
        Language::Ru => "Выйти",
        Language::En => "Quit",
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
