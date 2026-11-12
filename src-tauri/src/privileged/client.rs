//! Реальный клиент helper'а поверх Unix-сокета.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use splitroute_protocol::{
    parse_response, Command, ErrorCode, HelperStatus, ProtocolError, Request, MAX_MESSAGE_BYTES,
};

use super::{HelperClient, HelperClientError};

/// Таймаут по умолчанию на ответ helper'а.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Путь к сокету: env-переопределение, иначе dev/prod значение по умолчанию.
pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("SPLITROUTE_HELPER_SOCKET") {
        return PathBuf::from(path);
    }
    if cfg!(debug_assertions) {
        std::env::temp_dir().join("splitroute-helper.sock")
    } else {
        PathBuf::from("/var/run/splitroute/helper.sock")
    }
}

/// Клиент поверх Unix-сокета.
pub struct UnixHelperClient {
    socket_path: PathBuf,
    timeout: Duration,
}

impl UnixHelperClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn from_env() -> Self {
        Self::new(default_socket_path())
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    fn call(&self, command: Command, payload: Value) -> Result<Value, HelperClientError> {
        let stream = std::os::unix::net::UnixStream::connect(&self.socket_path).map_err(
            |error| match error.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                    HelperClientError::NotInstalled(error.to_string())
                }
                _ => HelperClientError::Unavailable(error.to_string()),
            },
        )?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|error| HelperClientError::Unavailable(error.to_string()))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|error| HelperClientError::Unavailable(error.to_string()))?;

        let request = Request::with_payload(next_id(), command, payload);
        let mut encoded = serde_json::to_vec(&request)
            .map_err(|error| HelperClientError::InvalidPayload(error.to_string()))?;
        encoded.push(b'\n');
        let mut writer = &stream;
        writer.write_all(&encoded).map_err(map_io_error)?;

        let mut response_line = String::new();
        let mut reader = BufReader::new(&stream).take(MAX_MESSAGE_BYTES as u64);
        reader.read_line(&mut response_line).map_err(map_io_error)?;
        if response_line.is_empty() {
            return Err(HelperClientError::Unavailable(
                "helper закрыл соединение без ответа".to_string(),
            ));
        }
        if !response_line.ends_with('\n') {
            return Err(HelperClientError::Protocol(ProtocolError::TooLarge {
                limit: MAX_MESSAGE_BYTES,
            }));
        }

        let response = parse_response(response_line.trim_end())?;
        if !response.ok {
            let (code, message) = response
                .error
                .map(|body| (body.code, body.message))
                .unwrap_or((ErrorCode::Internal, "helper не сообщил ошибку".to_string()));
            return Err(HelperClientError::Helper { code, message });
        }
        Ok(response.payload.unwrap_or(Value::Null))
    }
}

impl HelperClient for UnixHelperClient {
    fn status(&self) -> Result<HelperStatus, HelperClientError> {
        parse_status(self.call(Command::Status, Value::Null)?)
    }

    fn start(&self, config: Value) -> Result<HelperStatus, HelperClientError> {
        parse_status(self.call(Command::Start, serde_json::json!({ "config": config }))?)
    }

    fn stop(&self) -> Result<HelperStatus, HelperClientError> {
        parse_status(self.call(Command::Stop, Value::Null)?)
    }
}

fn parse_status(payload: Value) -> Result<HelperStatus, HelperClientError> {
    serde_json::from_value(payload)
        .map_err(|error| HelperClientError::InvalidPayload(error.to_string()))
}

fn next_id() -> String {
    let counter = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}-{counter:x}")
}

fn map_io_error(error: std::io::Error) -> HelperClientError {
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => HelperClientError::Timeout,
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
            HelperClientError::NotInstalled(error.to_string())
        }
        _ => HelperClientError::Unavailable(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    fn temp_socket(tag: &str) -> PathBuf {
        // Короткое имя: путь Unix-сокета ограничен SUN_LEN (~104 байта).
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000;
        std::env::temp_dir().join(format!("sr-{tag}-{}-{unique}.sock", std::process::id()))
    }

    /// Поднимает одноразовый сервер: читает запрос и отдаёт заранее заданный ответ.
    fn spawn_server(
        socket: PathBuf,
        response: Option<String>,
        delay: Duration,
    ) -> thread::JoinHandle<()> {
        let listener = UnixListener::bind(&socket).unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                thread::sleep(delay);
                if let Some(response) = response {
                    let _ = stream.write_all(response.as_bytes());
                }
            }
            let _ = std::fs::remove_file(&socket);
        })
    }

    fn ok_response(payload: Value) -> String {
        format!(
            "{}\n",
            serde_json::json!({"protocol": 1, "id": "x", "ok": true, "payload": payload})
        )
    }

    #[test]
    fn status_round_trip() {
        let socket = temp_socket("status");
        let handle = spawn_server(
            socket.clone(),
            Some(ok_response(serde_json::json!({
                "state": "running", "pid": 42, "version": "sing-box version 1.14.4"
            }))),
            Duration::ZERO,
        );
        let status = UnixHelperClient::new(socket).status().unwrap();
        assert_eq!(status.pid, Some(42));
        handle.join().unwrap();
    }

    #[test]
    fn version_mismatch_is_protocol_error() {
        let socket = temp_socket("version");
        let response = format!(
            "{}\n",
            serde_json::json!({"protocol": 2, "id": "x", "ok": true, "payload": {}})
        );
        let handle = spawn_server(socket.clone(), Some(response), Duration::ZERO);
        let error = UnixHelperClient::new(socket).status().unwrap_err();
        assert!(matches!(
            error,
            HelperClientError::Protocol(ProtocolError::VersionMismatch { .. })
        ));
        handle.join().unwrap();
    }

    #[test]
    fn helper_error_is_typed() {
        let socket = temp_socket("error");
        let response = format!(
            "{}\n",
            serde_json::json!({
                "protocol": 1, "id": "x", "ok": false,
                "error": {"code": "UNKNOWN_COMMAND", "message": "нет"}
            })
        );
        let handle = spawn_server(socket.clone(), Some(response), Duration::ZERO);
        let error = UnixHelperClient::new(socket).status().unwrap_err();
        assert!(matches!(
            error,
            HelperClientError::Helper {
                code: ErrorCode::UnknownCommand,
                ..
            }
        ));
        handle.join().unwrap();
    }

    #[test]
    fn missing_socket_is_not_installed() {
        let socket = temp_socket("missing");
        let error = UnixHelperClient::new(socket).status().unwrap_err();
        assert!(matches!(error, HelperClientError::NotInstalled(_)));
    }

    #[test]
    fn timeout_is_reported() {
        let socket = temp_socket("timeout");
        let handle = spawn_server(socket.clone(), None, Duration::from_millis(500));
        let error = UnixHelperClient::new(socket)
            .with_timeout(Duration::from_millis(50))
            .status()
            .unwrap_err();
        assert!(matches!(error, HelperClientError::Timeout));
        handle.join().unwrap();
    }

    #[test]
    fn empty_response_is_unavailable() {
        let socket = temp_socket("empty");
        let handle = spawn_server(socket.clone(), Some(String::new()), Duration::ZERO);
        let error = UnixHelperClient::new(socket).status().unwrap_err();
        assert!(matches!(error, HelperClientError::Unavailable(_)));
        handle.join().unwrap();
    }

    #[test]
    fn invalid_status_payload_is_reported() {
        let socket = temp_socket("payload");
        let handle = spawn_server(
            socket.clone(),
            Some(ok_response(serde_json::json!({"state": "что-то"}))),
            Duration::ZERO,
        );
        let error = UnixHelperClient::new(socket).status().unwrap_err();
        assert!(matches!(error, HelperClientError::InvalidPayload(_)));
        handle.join().unwrap();
    }

    #[test]
    fn request_id_is_unique() {
        let first = next_id();
        let second = next_id();
        assert_ne!(first, second);
    }

    #[test]
    fn oversized_response_is_rejected() {
        let socket = temp_socket("oversized");
        let payload = "x".repeat(MAX_MESSAGE_BYTES + 10);
        let handle = spawn_server(socket.clone(), Some(payload), Duration::ZERO);
        // Без завершающего \n ответ считается обрезанным лимитом.
        let error = UnixHelperClient::new(socket)
            .with_timeout(Duration::from_secs(2))
            .status()
            .unwrap_err();
        assert!(matches!(
            error,
            HelperClientError::Protocol(ProtocolError::TooLarge { .. })
                | HelperClientError::Timeout
                | HelperClientError::Unavailable(_)
        ));
        handle.join().unwrap();
    }
}
