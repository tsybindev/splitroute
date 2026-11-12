//! Установка/удаление helper'а.
//!
//! Реальная установка LaunchDaemon (macOS) и systemd (Linux) требует прав
//! администратора и делается на этапе 3. Здесь — интерфейс, который UI будет
//! вызывать, и честная заглушка вместо скрытой установки.

/// Ошибка установки helper'а.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InstallerError {
    #[error("для установки helper'а нужны права администратора (этап 3)")]
    RequiresPrivileges,
    #[error("установка helper'а не поддерживается на этой платформе")]
    Unsupported,
}

/// Установщик helper'а.
pub trait HelperInstaller: Send + Sync {
    fn install(&self) -> Result<(), InstallerError>;
    fn uninstall(&self) -> Result<(), InstallerError>;
}

/// Заглушка: установка отложена до этапа 3, вызывающий код получает явную ошибку.
pub struct UnsupportedInstaller;

impl HelperInstaller for UnsupportedInstaller {
    fn install(&self) -> Result<(), InstallerError> {
        Err(InstallerError::RequiresPrivileges)
    }

    fn uninstall(&self) -> Result<(), InstallerError> {
        Err(InstallerError::RequiresPrivileges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_installer_reports_privileges() {
        let installer = UnsupportedInstaller;
        assert_eq!(
            installer.install().unwrap_err(),
            InstallerError::RequiresPrivileges
        );
        assert_eq!(
            installer.uninstall().unwrap_err(),
            InstallerError::RequiresPrivileges
        );
    }
}
