//! Разбор запросов и формирование ответов протокола.
//!
//! Один JSON на строку, лимит сообщения — `MAX_MESSAGE_BYTES`. Команды берутся
//! из `splitroute-protocol`: helper не вводит собственных вариантов.

use std::io::{self, BufRead, Read, Write};

use serde_json::Value;
use splitroute_protocol::{Command, ErrorCode, Response, MAX_MESSAGE_BYTES, PROTOCOL_VERSION};

/// Разобранный запрос.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRequest {
    pub id: String,
    pub command: Command,
    pub payload: Value,
}

/// Ошибка разбора запроса вместе с id, который нужно вернуть клиенту.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcError {
    pub id: String,
    pub code: ErrorCode,
    pub message: String,
}

impl IpcError {
    pub fn new(id: impl Into<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            code,
            message: message.into(),
        }
    }

    pub fn into_response(self) -> Response {
        Response::error(self.id, self.code, self.message)
    }
}

/// Читает одно сообщение с ограничением размера.
///
/// Возвращает `Ok(None)` на чистом EOF. Сообщение без завершающего `\n`
/// принимается, если клиент закрыл соединение: так клиент может не ждать
/// сброса буфера.
pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Vec<u8>>, IpcError> {
    let mut buffer = Vec::new();
    let limit = (MAX_MESSAGE_BYTES + 1) as u64;
    let read = reader
        .by_ref()
        .take(limit)
        .read_until(b'\n', &mut buffer)
        .map_err(|error| {
            IpcError::new(
                "",
                ErrorCode::BadRequest,
                format!("ошибка чтения запроса: {error}"),
            )
        })?;

    if read == 0 {
        return Ok(None);
    }

    let terminated = buffer.last() == Some(&b'\n');
    if terminated {
        buffer.pop();
    }
    if buffer.len() > MAX_MESSAGE_BYTES {
        return Err(IpcError::new(
            "",
            ErrorCode::TooLarge,
            "сообщение превышает лимит 1 МиБ",
        ));
    }
    if buffer.iter().all(u8::is_ascii_whitespace) {
        return Err(IpcError::new("", ErrorCode::BadRequest, "пустое сообщение"));
    }

    Ok(Some(buffer))
}

/// Разбирает запрос и проверяет версию протокола до выполнения команды.
pub fn decode(raw: &[u8]) -> Result<ParsedRequest, IpcError> {
    let value: Value = serde_json::from_slice(raw)
        .map_err(|_| IpcError::new("", ErrorCode::BadRequest, "невалидный JSON"))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let object = value
        .as_object()
        .ok_or_else(|| IpcError::new(id.clone(), ErrorCode::BadRequest, "ожидался JSON-объект"))?;

    let protocol = object
        .get("protocol")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            IpcError::new(
                id.clone(),
                ErrorCode::BadRequest,
                "не указана версия протокола",
            )
        })?;
    if protocol != u64::from(PROTOCOL_VERSION) {
        return Err(IpcError::new(
            id,
            ErrorCode::ProtocolMismatch,
            format!("ожидается протокол {PROTOCOL_VERSION}, получен {protocol}"),
        ));
    }

    let command = object
        .get("command")
        .cloned()
        .ok_or_else(|| IpcError::new(id.clone(), ErrorCode::BadRequest, "не указана команда"))?;
    let command: Command = serde_json::from_value(command).map_err(|_| {
        IpcError::new(
            id.clone(),
            ErrorCode::UnknownCommand,
            "команда не разрешена",
        )
    })?;

    let payload = object.get("payload").cloned().unwrap_or(Value::Null);

    Ok(ParsedRequest {
        id,
        command,
        payload,
    })
}

/// Отправляет ответ одной JSON-строкой.
pub fn write_response<W: Write>(mut writer: W, response: &Response) -> io::Result<()> {
    let mut encoded = serde_json::to_vec(response).map_err(io::Error::other)?;
    encoded.push(b'\n');
    writer.write_all(&encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn valid_request_is_decoded() {
        let raw = br#"{"protocol":1,"id":"abc","command":"status"}"#;
        let request = decode(raw).unwrap();
        assert_eq!(request.id, "abc");
        assert_eq!(request.command, Command::Status);
        assert_eq!(request.payload, Value::Null);
    }

    #[test]
    fn start_payload_is_preserved() {
        let raw = br#"{"protocol":1,"id":"abc","command":"start","payload":{"config":{"log":{}}}}"#;
        let request = decode(raw).unwrap();
        assert_eq!(request.command, Command::Start);
        assert_eq!(request.payload["config"]["log"], serde_json::json!({}));
    }

    #[test]
    fn invalid_json_is_bad_request() {
        let error = decode(b"not json").unwrap_err();
        assert_eq!(error.code, ErrorCode::BadRequest);
        assert_eq!(error.id, "");
    }

    #[test]
    fn unknown_command_is_reported_with_id() {
        let raw = br#"{"protocol":1,"id":"u1","command":"exec"}"#;
        let error = decode(raw).unwrap_err();
        assert_eq!(error.code, ErrorCode::UnknownCommand);
        assert_eq!(error.id, "u1");
    }

    #[test]
    fn protocol_mismatch_is_reported() {
        let raw = br#"{"protocol":99,"id":"p1","command":"status"}"#;
        let error = decode(raw).unwrap_err();
        assert_eq!(error.code, ErrorCode::ProtocolMismatch);
        assert_eq!(error.id, "p1");
    }

    #[test]
    fn install_and_uninstall_are_known_commands() {
        for (wire, expected) in [
            ("install", Command::Install),
            ("uninstall", Command::Uninstall),
        ] {
            let raw = format!(r#"{{"protocol":1,"id":"x","command":"{wire}"}}"#);
            assert_eq!(decode(raw.as_bytes()).unwrap().command, expected);
        }
    }

    #[test]
    fn oversized_message_is_rejected() {
        let payload = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        let mut cursor = Cursor::new(payload);
        let error = read_message(&mut cursor).unwrap_err();
        assert_eq!(error.code, ErrorCode::TooLarge);
    }

    #[test]
    fn message_of_exact_limit_with_newline_is_accepted() {
        let mut payload = vec![b'x'; MAX_MESSAGE_BYTES];
        payload.push(b'\n');
        let mut cursor = Cursor::new(payload);
        let message = read_message(&mut cursor).unwrap().unwrap();
        assert_eq!(message.len(), MAX_MESSAGE_BYTES);
    }

    #[test]
    fn empty_message_is_bad_request() {
        let mut cursor = Cursor::new(b"\n".to_vec());
        let error = read_message(&mut cursor).unwrap_err();
        assert_eq!(error.code, ErrorCode::BadRequest);
    }

    #[test]
    fn clean_eof_returns_none() {
        let mut cursor = Cursor::new(Vec::new());
        assert_eq!(read_message(&mut cursor).unwrap(), None);
    }

    #[test]
    fn response_is_written_as_one_line() {
        let response = Response::ok("id", serde_json::json!({ "state": "stopped" }));
        let mut buffer = Vec::new();
        write_response(&mut buffer, &response).unwrap();
        assert_eq!(buffer.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert!(buffer.ends_with(b"\n"));
    }
}
