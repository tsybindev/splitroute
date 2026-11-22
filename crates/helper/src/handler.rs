//! Общая обработка запроса: разбор, диспетчеризация и ответ.
//!
//! Не зависит от ОС: транспорт (Unix-сокет или named pipe) отдаёт готовый
//! поток, здесь выполняется только протокол. Аутентификация клиента — дело
//! транспорта, чтобы Unix и Windows проверяли её по-своему.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use splitroute_protocol::{Command, ErrorCode, ReloadRoutingPayload, Response, StartPayload};

use crate::core::{HelperCore, HelperError};
use crate::ipc::{self, ParsedRequest};

/// Читает один запрос, выполняет команду и пишет ответ.
///
/// `Ok(())` — соединение обработано (в том числе когда ответ — ошибка).
pub fn serve<B: BufRead, W: Write>(
    mut reader: B,
    writer: &mut W,
    shared: &Arc<Mutex<HelperCore>>,
    shutting_down: bool,
) -> std::io::Result<()> {
    let raw = match ipc::read_message(&mut reader) {
        Ok(Some(raw)) => raw,
        Ok(None) => return Ok(()),
        Err(error) => return ipc::write_response(writer, &error.into_response()),
    };

    let request = match ipc::decode(&raw) {
        Ok(request) => request,
        Err(error) => return ipc::write_response(writer, &error.into_response()),
    };

    let response = dispatch(&request, shared, shutting_down);
    ipc::write_response(writer, &response)
}

/// Выполняет разрешённую команду. Установка и удаление сервиса — не по сокету.
pub fn dispatch(
    request: &ParsedRequest,
    shared: &Arc<Mutex<HelperCore>>,
    shutting_down: bool,
) -> Response {
    let id = request.id.clone();
    let result = match request.command {
        Command::Status => {
            let mut core = lock(shared);
            Ok(core.status_value())
        }
        Command::Start => {
            let mut core = lock(shared);
            if shutting_down {
                return Response::error(id, ErrorCode::Internal, "helper завершается");
            }
            let start: StartPayload = match serde_json::from_value(request.payload.clone()) {
                Ok(start) => start,
                Err(_) => {
                    return Response::error(
                        id,
                        ErrorCode::BadRequest,
                        "payload: ожидались config, rule_set и kill_switch",
                    );
                }
            };
            core.start(start.config, start.rule_set, start.kill_switch)
                .map(status_payload)
        }
        Command::ReloadRouting => {
            let mut core = lock(shared);
            if shutting_down {
                return Response::error(id, ErrorCode::Internal, "helper завершается");
            }
            let reload: ReloadRoutingPayload = match serde_json::from_value(request.payload.clone())
            {
                Ok(reload) => reload,
                Err(_) => {
                    return Response::error(
                        id,
                        ErrorCode::BadRequest,
                        "payload: ожидался rule_set",
                    );
                }
            };
            core.reload_routing(reload.rule_set).map(status_payload)
        }
        Command::Stop => {
            let mut core = lock(shared);
            Ok(status_payload(core.stop()))
        }
        // Установка и удаление сервиса требуют прав администратора и делаются
        // установщиком; по сокету/пайпу эти команды запрещены.
        Command::Install | Command::Uninstall => Err(HelperError::new(
            ErrorCode::Forbidden,
            "команда выполняется установщиком с правами администратора",
        )),
    };

    match result {
        Ok(payload) => Response::ok(id, payload),
        Err(error) => Response::error(id, error.code, error.message),
    }
}

pub fn status_payload(status: splitroute_protocol::HelperStatus) -> Value {
    serde_json::to_value(status).unwrap_or(Value::Null)
}

pub fn lock(shared: &Arc<Mutex<HelperCore>>) -> MutexGuard<'_, HelperCore> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
