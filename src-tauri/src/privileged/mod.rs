//! Клиент и установщик привилегированного helper'а.
//!
//! Транспорт — Unix-сокет, JSON по одной строке. Установка сервиса на macOS —
//! LaunchDaemon через `MacosInstaller`; на остальных платформах пока заглушка.

pub mod installer;
pub mod mock;

#[cfg(unix)]
pub mod client;

#[cfg(target_os = "macos")]
pub mod macos;

use serde_json::Value;
use splitroute_protocol::{ErrorCode, HelperStatus, ProtocolError};

pub use installer::{HelperInstaller, InstallerError, UnsupportedInstaller};
pub use mock::MockHelperClient;

#[cfg(unix)]
pub use client::{default_socket_path, UnixHelperClient};

#[cfg(target_os = "macos")]
pub use macos::MacosInstaller;

/// Ошибка клиента helper'а (не зависит от платформы).
#[derive(Debug, thiserror::Error)]
pub enum HelperClientError {
    #[error("helper не установлен или сокет не найден: {0}")]
    NotInstalled(String),
    #[error("helper недоступен: {0}")]
    Unavailable(String),
    #[error("таймаут ответа helper'а")]
    Timeout,
    #[error("ошибка протокола: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("helper вернул ошибку {code:?}: {message}")]
    Helper { code: ErrorCode, message: String },
    #[error("некорректный ответ helper'а: {0}")]
    InvalidPayload(String),
}

/// Минимальный набор операций над ядром через helper.
pub trait HelperClient: Send + Sync {
    fn status(&self) -> Result<HelperStatus, HelperClientError>;
    fn start(&self, config: Value) -> Result<HelperStatus, HelperClientError>;
    fn stop(&self) -> Result<HelperStatus, HelperClientError>;
}
