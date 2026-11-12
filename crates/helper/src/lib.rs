//! Библиотека привилегированного helper'а SplitRoute.
//!
//! Helper принимает только команды `start`, `stop` и `status`. Установка и
//! удаление сервиса выполняются установщиком с правами администратора, а не
//! по IPC (AGENTS.md §10).
//!
//! Транспорт платформенный: Unix-сокет (`server`) или named pipe (`pipe`).
//! Протокол общий: один JSON на строку, типы — из `splitroute-protocol`,
//! дублировать структуры нельзя.

pub mod args;
pub mod core;
pub mod handler;
pub mod ipc;
pub mod platform;

#[cfg(windows)]
pub mod pipe;
#[cfg(unix)]
pub mod server;
#[cfg(windows)]
pub mod service;

pub use core::{HelperCore, HelperError};

#[cfg(windows)]
pub use pipe::{request_shutdown, run, ServerOptions};
#[cfg(unix)]
pub use server::{resolve_allowed_uid, run, ServerOptions};
