//! Тонкие Tauri commands: без бизнес-логики, только вызов клиента/установщика.

use std::sync::Arc;

use serde_json::Value;
use splitroute_protocol::HelperStatus;

use crate::privileged::{HelperClient, HelperClientError, HelperInstaller, InstallerError};

/// Состояние приложения, доступное командам.
pub struct AppState {
    pub helper: Arc<dyn HelperClient>,
    pub installer: Arc<dyn HelperInstaller>,
}

impl AppState {
    pub fn new(helper: Arc<dyn HelperClient>, installer: Arc<dyn HelperInstaller>) -> Self {
        Self { helper, installer }
    }
}

fn client_error(error: HelperClientError) -> String {
    error.to_string()
}

fn installer_error(error: InstallerError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn helper_status(state: tauri::State<'_, AppState>) -> Result<HelperStatus, String> {
    state.helper.status().map_err(client_error)
}

#[tauri::command]
pub fn connect(state: tauri::State<'_, AppState>, config: Value) -> Result<HelperStatus, String> {
    state.helper.start(config).map_err(client_error)
}

#[tauri::command]
pub fn disconnect(state: tauri::State<'_, AppState>) -> Result<HelperStatus, String> {
    state.helper.stop().map_err(client_error)
}

#[tauri::command]
pub fn helper_install(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.installer.install().map_err(installer_error)
}

#[tauri::command]
pub fn helper_uninstall(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.installer.uninstall().map_err(installer_error)
}
