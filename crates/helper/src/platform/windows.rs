//! Windows-примитивы helper'а.
//!
//! Права на runtime-каталог задаёт установщик через ACL named pipe и каталога:
//! доступ получают только SYSTEM, Administrators и SID установившего
//! пользователя. Helper не умеет менять ACL сам — иначе он мог бы расширить
//! доступ по запросу клиента.
//!
//! Ядро завершается штатным `kill`: sing-box на Windows снимает wintun-адаптер
//! при завершении процесса. Мягкий SIGTERM недоступен, поэтому таймаут не
//! используется.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::process::Child;
use std::time::Duration;

/// Под Windows helper всегда работает как служба под SYSTEM.
pub fn is_root() -> bool {
    true
}

/// Создаёт файл; доступ ограничен ACL каталога, который выставил установщик.
pub fn open_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
}

/// Открывает лог ядра на дозапись; доступ ограничен ACL каталога.
pub fn open_log(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// Права каталога задаёт установщик через ACL — менять их отсюда нельзя.
pub fn set_dir_mode(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Права файла задаёт ACL каталога — отдельных unix-режимов нет.
pub fn set_file_mode(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Останавливает ядро: `kill` завершает процесс, sing-box убирает wintun-адаптер.
pub fn terminate(child: &mut Child, _timeout: Duration) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}
