//! Платформенные примитивы helper'а: права файлов, привилегии и остановка ядра.
//!
//! Сквозные части (`core`, `ipc`) не знают об ОС: здесь только то, что
//! действительно различается. Unix-ветка сохраняет прежнее поведение
//! (`0711`/`0600`, SIGTERM → SIGKILL); Windows-ветка полагается на ACL, который
//! выставил установщик службы.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{is_root, open_log, open_private, set_dir_mode, set_file_mode, terminate};
#[cfg(windows)]
pub use windows::{is_root, open_log, open_private, set_dir_mode, set_file_mode, terminate};
