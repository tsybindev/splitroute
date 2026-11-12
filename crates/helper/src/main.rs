//! Скелет привилегированного helper'а.
//!
//! Реализация (Unix socket / named pipe, проверка uid, start/stop/status,
//! установка сервиса) — этап 3. Прототип протокола и механики: `spike/helper/`.
//! Здесь пока только каркас, чтобы workspace собирался целиком.

use splitroute_protocol::PROTOCOL_VERSION;

fn main() {
    println!("splitroute-helper: скелет, протокол {PROTOCOL_VERSION} (реализация — этап 3)");
}
