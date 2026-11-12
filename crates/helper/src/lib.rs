//! Библиотека привилегированного helper'а SplitRoute.
//!
//! Helper принимает только команды `start`, `stop` и `status` по Unix-сокету.
//! Установка и удаление сервиса выполняются установщиком с правами
//! администратора, а не по сокету (AGENTS.md §10).
//!
//! Протокол — один JSON на строку, тип сообщений берётся из
//! `splitroute-protocol`, дублировать структуры нельзя.

#![cfg(unix)]

pub mod core;
pub mod ipc;
pub mod server;

pub use core::{HelperCore, HelperError};
pub use server::{resolve_allowed_uid, run, ServerOptions};
