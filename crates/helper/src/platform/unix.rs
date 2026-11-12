//! Unix-примитивы helper'а.
//!
//! Под root runtime-каталог доступен на обход (`0711`), чтобы пользователь с
//! `--allow-uid` мог дойти до своего `0600`-сокета, но не мог прочитать
//! содержимое. Под обычным пользователем каталог остаётся `0700`.

use std::fs::{File, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

/// Признак работы под root: под root каталог получает права `0711`.
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Права runtime-каталога: `0711` под root (обход без листинга), иначе `0700`.
pub fn dir_mode(root: bool) -> u32 {
    if root {
        0o711
    } else {
        0o700
    }
}

/// Создаёт файл с правами `0600`.
pub fn open_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

/// Открывает лог ядра на дозапись с правами `0600`.
pub fn open_log(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

/// Выставляет права каталога (`0711` под root, иначе `0700`).
pub fn set_dir_mode(path: &Path) -> io::Result<()> {
    let mode = dir_mode(is_root());
    std::fs::set_permissions(path, Permissions::from_mode(mode))
}

/// Выставляет права файла `0600`.
pub fn set_file_mode(path: &Path) -> io::Result<()> {
    std::fs::set_permissions(path, Permissions::from_mode(0o600))
}

/// Останавливает ядро: SIGTERM даёт убрать TUN и маршруты, SIGKILL — крайняя мера.
pub fn terminate(child: &mut Child, timeout: Duration) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    // SIGTERM даёт ядру убрать TUN и маршруты; SIGKILL — только крайняя мера.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_gets_traversable_directory() {
        assert_eq!(dir_mode(true), 0o711);
        assert_eq!(dir_mode(false), 0o700);
    }

    #[test]
    fn private_file_has_0600() {
        let dir = std::env::temp_dir().join(format!("sr-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let file = open_private(&path).unwrap();
        drop(file);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
