//! Общие типы SplitRoute: IPC «приложение ↔ helper» и доменные модели.
//!
//! Этот crate — единственный источник типов, которыми обмениваются UI-ядро и
//! привилегированный helper. Дублировать структуры в других местах нельзя
//! (AGENTS.md §2, §10).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod model;

pub use model::{
    AppRule, AppRuleView, DnsMode, InstalledApp, Language, LogLevel, Platform, Profile, Protocol,
    RunningApp, Settings, TlsConfig, TlsKind, Transport,
};

/// Версия протокола IPC. Несовпадение — повод переустановить helper.
///
/// Версия 2 добавила `kill_switch` в `start` и состояние kill switch в `status`.
/// Версия 3 добавила `rule_set` в `start` и команду `reload_routing`: per-app
/// правила применяются к работающему ядру через локальный rule-set без
/// рестарта. Старый helper не знал бы новых полей и команд, поэтому
/// совместимость не сохраняется.
pub const PROTOCOL_VERSION: u16 = 3;

/// Максимальный размер одного сообщения протокола, байт.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Разрешённые команды helper'а. Других команд быть не должно.
///
/// `snake_case`, а не `lowercase`: `lowercase` не разделяет слова и дал бы
/// `reloadrouting`, а проводной формат однословных команд (`start`, `status`)
/// при смене не меняется.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
    ReloadRouting,
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

/// Payload команды `start`: конфиг ядра, rule-set per-app правил и kill switch.
///
/// `rule_set` — содержимое локального rule-set (`route.rule_set` в конфиге
/// ссылается на файл по тегу `splitroute-apps`); helper записывает его в
/// runtime-каталог и подставляет путь в конфиг, чтобы приложение не знало
/// файловых путей privileged-каталога.
///
/// `kill_switch` передаётся отдельно от конфига: правила ставит helper, а не
/// sing-box, и они должны оставаться в силе, если ядро упадёт.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartPayload {
    pub config: Value,
    pub rule_set: Value,
    #[serde(default)]
    pub kill_switch: bool,
}

/// Payload команды `reload_routing`: новый rule-set per-app правил.
///
/// Вызывается при правках правил активной сессии: helper атомарно заменяет
/// файл rule-set, sing-box перечитывает его без рестарта (локальные rule-set
/// перечитываются при изменении файла с sing-box 1.10.0). Kill switch и
/// DNS-режим при этом не затрагиваются.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReloadRoutingPayload {
    pub rule_set: Value,
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
    ReloadFailed,
    KillSwitchFailed,
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

/// Состояние kill switch в helper'е.
///
/// `enabled` — правила сейчас установлены и защита действует (в том числе когда
/// ядро уже упало). `error` — правила не удалось снять/поставить.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KillSwitchStatus {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

/// Ответ helper'а на `status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelperStatus {
    pub state: CoreState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "deserialize_optional_secs"
    )]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub kill_switch: KillSwitchStatus,
}

/// Принимает время старта как целые или дробные секунды: dev-helper отдаёт
/// `time.time()` (float), продакшн-helper может отдавать целые.
fn deserialize_optional_secs<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<f64>::deserialize(deserializer)?;
    Ok(value
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| seconds.trunc() as u64))
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
            (Command::ReloadRouting, "reload_routing"),
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
        let foreign = PROTOCOL_VERSION + 1;
        let raw = format!(r#"{{"protocol":{foreign},"id":"x","ok":true,"payload":{{}}}}"#);
        assert_eq!(
            parse_response(&raw),
            Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: foreign
            })
        );
    }

    #[test]
    fn parse_response_rejects_previous_protocol_version() {
        // v1 helper не знает kill switch: считать его совместимым нельзя.
        let raw = r#"{"protocol":1,"id":"x","ok":true,"payload":{}}"#;
        assert_eq!(
            parse_response(raw),
            Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: 1
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
            kill_switch: KillSwitchStatus {
                enabled: true,
                error: None,
            },
        };
        let encoded = serde_json::to_string(&status).unwrap();
        let decoded: HelperStatus = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, status);
        assert!(encoded.contains(r#""kill_switch""#));
    }

    #[test]
    fn helper_status_accepts_fractional_started_at() {
        // dev-helper на Python отдаёт time.time(): дробные секунды.
        let raw = r#"{"state":"running","pid":42,"started_at":1762950000.123}"#;
        let decoded: HelperStatus = serde_json::from_str(raw).unwrap();
        assert_eq!(decoded.started_at, Some(1_762_950_000));
    }

    #[test]
    fn helper_status_without_kill_switch_reads_as_disabled() {
        // Поле появилось в протоколе 2; отсутствие — «выключен», а не ошибка.
        let raw = r#"{"state":"stopped"}"#;
        let decoded: HelperStatus = serde_json::from_str(raw).unwrap();
        assert!(!decoded.kill_switch.enabled);
        assert!(decoded.kill_switch.error.is_none());
    }

    #[test]
    fn start_payload_defaults_kill_switch_to_false() {
        let payload: StartPayload = serde_json::from_value(serde_json::json!({
            "config": { "log": { "level": "info" } },
            "rule_set": { "version": 2, "rules": [] }
        }))
        .unwrap();
        assert!(!payload.kill_switch);
        assert_eq!(payload.config["log"]["level"], "info");
        assert_eq!(payload.rule_set["version"], 2);
    }

    #[test]
    fn start_payload_round_trip_with_kill_switch() {
        let payload = StartPayload {
            config: serde_json::json!({ "log": { "level": "info" } }),
            rule_set: serde_json::json!({ "version": 2, "rules": [] }),
            kill_switch: true,
        };
        let encoded = serde_json::to_string(&payload).unwrap();
        let decoded: StartPayload = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, payload);
        assert!(encoded.contains(r#""kill_switch":true"#));
        assert!(encoded.contains(r#""rule_set""#));
    }

    #[test]
    fn reload_routing_payload_round_trip() {
        let payload = ReloadRoutingPayload {
            rule_set: serde_json::json!({
                "version": 2,
                "rules": [{ "process_name": ["Discord.exe"] }]
            }),
        };
        let encoded = serde_json::to_string(&payload).unwrap();
        let decoded: ReloadRoutingPayload = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn kill_switch_error_code_is_stable() {
        assert_eq!(
            serde_json::to_value(ErrorCode::KillSwitchFailed).unwrap(),
            "KILL_SWITCH_FAILED"
        );
    }

    #[test]
    fn reload_error_code_is_stable() {
        assert_eq!(
            serde_json::to_value(ErrorCode::ReloadFailed).unwrap(),
            "RELOAD_FAILED"
        );
    }

    #[test]
    fn parse_response_rejects_protocol_v2() {
        // v2 helper не знает rule_set и reload_routing: совместимость нельзя
        // сохранять, иначе правки правил молча перестанут применяться.
        let raw = r#"{"protocol":2,"id":"x","ok":true,"payload":{}}"#;
        assert_eq!(
            parse_response(raw),
            Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: 2
            })
        );
    }
}
