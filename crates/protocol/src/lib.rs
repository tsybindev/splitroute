//! Общие типы SplitRoute: IPC «приложение ↔ helper» и доменные модели.
//!
//! Этот crate — единственный источник типов, которыми обмениваются UI-ядро и
//! привилегированный helper. Дублировать структуры в других местах нельзя
//! (AGENTS.md §2, §10).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod model;

pub use model::{
    AppRule, DnsMode, LogLevel, Platform, Profile, Protocol, Settings, TlsConfig, TlsKind,
    Transport,
};

/// Версия протокола IPC. Несовпадение — повод переустановить helper.
pub const PROTOCOL_VERSION: u16 = 1;

/// Максимальный размер одного сообщения протокола, байт.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Разрешённые команды helper'а. Других команд быть не должно.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Command {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
}

/// Запрос к helper'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol: u16,
    pub id: String,
    pub command: Command,
    #[serde(default)]
    pub payload: Value,
}

impl Request {
    pub fn new(id: impl Into<String>, command: Command) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id: id.into(),
            command,
            payload: Value::Null,
        }
    }

    pub fn with_payload(id: impl Into<String>, command: Command, payload: Value) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id: id.into(),
            command,
            payload,
        }
    }

    /// Проверяет совместимость версии протокола.
    pub fn check_version(&self) -> Result<(), ProtocolError> {
        if self.protocol == PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: self.protocol,
            })
        }
    }
}

/// Код ошибки протокола: стабильный, машинно-читаемый.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    BadRequest,
    Forbidden,
    TooLarge,
    ProtocolMismatch,
    UnknownCommand,
    ConfigInvalid,
    AlreadyRunning,
    StartFailed,
    Internal,
}

/// Тело ошибки в ответе helper'а.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

/// Ответ helper'а.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub protocol: u16,
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<ErrorBody>,
}

impl Response {
    pub fn ok(id: impl Into<String>, payload: Value) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id: id.into(),
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn error(id: impl Into<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id: id.into(),
            ok: false,
            payload: None,
            error: Some(ErrorBody {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Ошибки разбора и проверки протокола.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("несовпадение версии протокола: ожидается {expected}, получено {got}")]
    VersionMismatch { expected: u16, got: u16 },
    #[error("сообщение превышает лимит {limit} байт")]
    TooLarge { limit: usize },
    #[error("невалидный JSON: {0}")]
    InvalidJson(String),
}

/// Состояние процесса ядра.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoreState {
    Running,
    Stopped,
}

/// Состояние подключения в UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

/// Ответ helper'а на `status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelperStatus {
    pub state: CoreState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<u64>,
}

/// Разбор ответа с проверкой версии и статуса.
pub fn parse_response(raw: &str) -> Result<Response, ProtocolError> {
    if raw.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge {
            limit: MAX_MESSAGE_BYTES,
        });
    }
    let response: Response =
        serde_json::from_str(raw).map_err(|error| ProtocolError::InvalidJson(error.to_string()))?;
    if response.protocol != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            expected: PROTOCOL_VERSION,
            got: response.protocol,
        });
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_protocol_version() {
        let request = Request::new("abc", Command::Start);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["protocol"], PROTOCOL_VERSION);
        assert_eq!(json["id"], "abc");
        assert_eq!(json["command"], "start");
    }

    #[test]
    fn all_commands_serialize_lowercase() {
        for (command, expected) in [
            (Command::Install, "install"),
            (Command::Uninstall, "uninstall"),
            (Command::Start, "start"),
            (Command::Stop, "stop"),
            (Command::Status, "status"),
        ] {
            assert_eq!(serde_json::to_value(command).unwrap(), expected);
        }
    }

    #[test]
    fn request_round_trip_preserves_payload() {
        let request = Request::with_payload(
            "id-1",
            Command::Start,
            serde_json::json!({"config": {"log": {"level": "info"}}}),
        );
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: Request = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn version_mismatch_is_rejected() {
        let mut request = Request::new("id", Command::Status);
        request.protocol = 99;
        assert_eq!(
            request.check_version(),
            Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: 99
            })
        );
    }

    #[test]
    fn response_error_has_code_and_no_payload() {
        let response = Response::error("id", ErrorCode::UnknownCommand, "нет такой команды");
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "UNKNOWN_COMMAND");
        assert!(json.get("payload").is_none());
    }

    #[test]
    fn parse_response_rejects_foreign_version() {
        let raw = r#"{"protocol":2,"id":"x","ok":true,"payload":{}}"#;
        assert_eq!(
            parse_response(raw),
            Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: 2
            })
        );
    }

    #[test]
    fn parse_response_rejects_oversized_message() {
        let raw = "x".repeat(MAX_MESSAGE_BYTES + 1);
        assert!(matches!(
            parse_response(&raw),
            Err(ProtocolError::TooLarge { .. })
        ));
    }

    #[test]
    fn helper_status_round_trip() {
        let status = HelperStatus {
            state: CoreState::Running,
            pid: Some(4242),
            version: Some("sing-box version 1.14.4".to_string()),
            started_at: Some(1_762_950_000),
        };
        let encoded = serde_json::to_string(&status).unwrap();
        let decoded: HelperStatus = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, status);
    }
}
