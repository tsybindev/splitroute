//! Установка и удаление helper'а.
//!
//! На macOS установщик кладёт бинарь в `/Library/PrivilegedHelperTools`, а plist —
//! в `/Library/LaunchDaemons`, после чего регистрирует сервис через `launchctl`.
//! Пароль администратора запрашивает система (окно авторизации), сам установщик
//! его не хранит. На остальных платформах установка пока не поддерживается.

/// Ошибка установки helper'а.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InstallerError {
    #[error("для установки helper'а нужны права администратора")]
    RequiresPrivileges,
    #[error("установка helper'а не поддерживается на этой платформе")]
    Unsupported,
    #[error("не найден файл: {0}")]
    MissingBinary(String),
    #[error("не удалось установить helper: {0}")]
    Failed(String),
}

/// Установщик helper'а.
///
/// Подтверждение установки даёт системный запрос прав; UI не хранит пароль.
pub trait HelperInstaller: Send + Sync {
    fn install(&self) -> Result<(), InstallerError>;
    fn uninstall(&self) -> Result<(), InstallerError>;
}

/// Заглушка для платформ, где установщик ещё не реализован.
pub struct UnsupportedInstaller;

impl HelperInstaller for UnsupportedInstaller {
    fn install(&self) -> Result<(), InstallerError> {
        Err(InstallerError::Unsupported)
    }

    fn uninstall(&self) -> Result<(), InstallerError> {
        Err(InstallerError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_installer_reports_unsupported_platform() {
        let installer = UnsupportedInstaller;
        assert_eq!(
            installer.install().unwrap_err(),
            InstallerError::Unsupported
        );
        assert_eq!(
            installer.uninstall().unwrap_err(),
            InstallerError::Unsupported
        );
    }
}
