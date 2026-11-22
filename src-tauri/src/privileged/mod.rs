//! Клиент и установщик привилегированного helper'а.
//!
//! Транспорт — Unix-сокет, JSON по одной строке. Установка сервиса: на macOS —
//! LaunchDaemon через `MacosInstaller`, на Linux — systemd-юнит через
//! `LinuxInstaller`; на остальных платформах пока заглушка.

pub mod installer;
pub mod mock;

// Клиент собирается на всех ОС: протокол общий, транспорт выбирается внутри
// по `cfg` (Unix-сокет или named pipe), тесты обмена гоняются на любой ОС.
pub mod client;

#[cfg(target_os = "macos")]
pub mod macos;

// Windows-установщик тоже собирается в тестовой конфигурации: генерация команд
// `sc.exe`, bat-скриптов и путей — чистые функции, тесты гоняются на любой ОС.
#[cfg(any(target_os = "windows", test))]
pub mod windows;

// Linux-установщик собирается и в тестовой конфигурации: генерация юнита и
// скриптов — чистые функции, тесты гоняются на любой ОС.
#[cfg(any(target_os = "linux", test))]
pub mod linux;

use serde_json::Value;
use splitroute_protocol::{ErrorCode, HelperStatus, ProtocolError};

pub use installer::{HelperInstaller, InstallerError, UnsupportedInstaller};
pub use mock::MockHelperClient;

#[cfg(unix)]
pub use client::{default_socket_path, UnixHelperClient};

#[cfg(windows)]
pub use client::{default_pipe_path, WindowsPipeClient};

#[cfg(target_os = "macos")]
pub use macos::MacosInstaller;

#[cfg(target_os = "windows")]
pub use windows::WindowsInstaller;

#[cfg(target_os = "linux")]
pub use linux::LinuxInstaller;

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
    fn start(
        &self,
        config: Value,
        rule_set: Value,
        kill_switch: bool,
    ) -> Result<HelperStatus, HelperClientError>;
    /// Правки per-app правил к активной сессии: helper заменяет файл rule-set,
    /// ядро перечитывает его без рестарта (S10-T1).
    fn reload_routing(&self, rule_set: Value) -> Result<HelperStatus, HelperClientError>;
    /// Рестарт ядра с новым конфигом без снятия правил kill switch (S12-T1):
    /// переключение локации не открывает окно fail-open. Payload тот же, что
    /// у `start`; при неудаче helper оставляет состояние как после `stop`.
    fn restart(
        &self,
        config: Value,
        rule_set: Value,
        kill_switch: bool,
    ) -> Result<HelperStatus, HelperClientError>;
    fn stop(&self) -> Result<HelperStatus, HelperClientError>;
}
