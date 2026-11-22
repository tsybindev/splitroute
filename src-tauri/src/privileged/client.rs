//! Клиент helper'а: протокол общий, транспорт платформенный.
//!
//! Один JSON на строку, ответ читается до `\n`, лимит — `MAX_MESSAGE_BYTES`.
//! Unix использует Unix-сокет, Windows — named pipe. Обмен сообщениями
//! (`call` и обёртки) один и тот же, поэтому проверяется на любой ОС.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use splitroute_protocol::{
    parse_response, Command, ErrorCode, HelperStatus, ProtocolError, ReloadRoutingPayload, Request,
    StartPayload, MAX_MESSAGE_BYTES,
};

use super::HelperClientError;

/// Таймаут по умолчанию на ответ helper'а.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Имя named pipe helper'а без префикса (совпадает с `--pipe` и установщиком).
#[cfg(windows)]
pub const PIPE_NAME: &str = "splitroute-helper";

/// Путь к сокету: env-переопределение, иначе dev/prod значение по умолчанию.
///
/// Linux: `/run/splitroute/helper.sock` — этот путь задаёт systemd-юнит.
/// macOS: `/var/run/splitroute/helper.sock` — путь из LaunchDaemon.
#[cfg(unix)]
pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("SPLITROUTE_HELPER_SOCKET") {
        return PathBuf::from(path);
    }
    if cfg!(debug_assertions) {
        std::env::temp_dir().join("splitroute-helper.sock")
    } else if cfg!(target_os = "linux") {
        PathBuf::from("/run/splitroute/helper.sock")
    } else {
        PathBuf::from("/var/run/splitroute/helper.sock")
    }
}

/// Путь к named pipe службы Windows.
#[cfg(windows)]
pub fn default_pipe_path() -> PathBuf {
    match std::env::var("SPLITROUTE_HELPER_PIPE") {
        Ok(path) => PathBuf::from(path),
        Err(_) => PathBuf::from(format!(r"\\.\pipe\{PIPE_NAME}")),
    }
}

/// Отправляет один запрос и разбирает ответ на уже открытом потоке.
///
/// Таймауты выставляет транспорт: у `File` их нет, у сокета есть.
pub fn call<S: Read + Write>(
    stream: &mut S,
    command: Command,
    payload: Value,
) -> Result<Value, HelperClientError> {
    let request = Request::with_payload(next_id(), command, payload);
    let mut encoded = serde_json::to_vec(&request)
        .map_err(|error| HelperClientError::InvalidPayload(error.to_string()))?;
    encoded.push(b'\n');
    stream.write_all(&encoded).map_err(map_io_error)?;

    let mut response_line = String::new();
    let mut reader = BufReader::new(stream).take(MAX_MESSAGE_BYTES as u64);
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

/// `status` поверх открытого потока.
pub fn status<S: Read + Write>(stream: &mut S) -> Result<HelperStatus, HelperClientError> {
    parse_status(call(stream, Command::Status, Value::Null)?)
}

/// `start` поверх открытого потока. `kill_switch` — часть протокола 2,
/// `rule_set` — часть протокола 3.
pub fn start<S: Read + Write>(
    stream: &mut S,
    config: Value,
    rule_set: Value,
    kill_switch: bool,
) -> Result<HelperStatus, HelperClientError> {
    let payload = serde_json::to_value(StartPayload {
        config,
        rule_set,
        kill_switch,
    })
    .map_err(|error| HelperClientError::InvalidPayload(error.to_string()))?;
    parse_status(call(stream, Command::Start, payload)?)
}

/// `reload_routing` поверх открытого потока: правки per-app правил к активной
/// сессии без рестарта ядра (S10-T1).
pub fn reload_routing<S: Read + Write>(
    stream: &mut S,
    rule_set: Value,
) -> Result<HelperStatus, HelperClientError> {
    let payload = serde_json::to_value(ReloadRoutingPayload { rule_set })
        .map_err(|error| HelperClientError::InvalidPayload(error.to_string()))?;
    parse_status(call(stream, Command::ReloadRouting, payload)?)
}

/// `stop` поверх открытого потока.
pub fn stop<S: Read + Write>(stream: &mut S) -> Result<HelperStatus, HelperClientError> {
    parse_status(call(stream, Command::Stop, Value::Null)?)
}

/// `restart` поверх открытого потока: рестарт ядра с новым конфигом без снятия
/// правил kill switch (S12-T1). Payload тот же, что у `start`.
pub fn restart<S: Read + Write>(
    stream: &mut S,
    config: Value,
    rule_set: Value,
    kill_switch: bool,
) -> Result<HelperStatus, HelperClientError> {
    let payload = serde_json::to_value(StartPayload {
        config,
        rule_set,
        kill_switch,
    })
    .map_err(|error| HelperClientError::InvalidPayload(error.to_string()))?;
    parse_status(call(stream, Command::Restart, payload)?)
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

#[cfg(unix)]
mod unix {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use serde_json::Value;
    use splitroute_protocol::HelperStatus;

    use super::{default_socket_path, map_io_error, DEFAULT_TIMEOUT};
    use crate::privileged::{HelperClient, HelperClientError};

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

        fn connect(&self) -> Result<std::os::unix::net::UnixStream, HelperClientError> {
            let stream =
                std::os::unix::net::UnixStream::connect(&self.socket_path).map_err(|error| {
                    match error.kind() {
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                            HelperClientError::NotInstalled(error.to_string())
                        }
                        _ => HelperClientError::Unavailable(error.to_string()),
                    }
                })?;
            stream
                .set_read_timeout(Some(self.timeout))
                .map_err(map_io_error)?;
            stream
                .set_write_timeout(Some(self.timeout))
                .map_err(map_io_error)?;
            Ok(stream)
        }
    }

    impl HelperClient for UnixHelperClient {
        fn status(&self) -> Result<HelperStatus, HelperClientError> {
            super::status(&mut self.connect()?)
        }

        fn start(
            &self,
            config: Value,
            rule_set: Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            super::start(&mut self.connect()?, config, rule_set, kill_switch)
        }

        fn reload_routing(&self, rule_set: Value) -> Result<HelperStatus, HelperClientError> {
            super::reload_routing(&mut self.connect()?, rule_set)
        }

        fn restart(
            &self,
            config: Value,
            rule_set: Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            super::restart(&mut self.connect()?, config, rule_set, kill_switch)
        }

        fn stop(&self) -> Result<HelperStatus, HelperClientError> {
            super::stop(&mut self.connect()?)
        }
    }
}

#[cfg(unix)]
pub use unix::UnixHelperClient;

#[cfg(windows)]
mod windows_pipe {
    use std::fs::{File, OpenOptions};
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use splitroute_protocol::HelperStatus;

    use super::default_pipe_path;
    use crate::privileged::{HelperClient, HelperClientError};

    /// Клиент поверх named pipe службы Windows.
    ///
    /// Pipe открывается как обычный файл — `std` на Windows это поддерживает.
    /// Таймаута у `File` нет, но helper отвечает и закрывает соединение,
    /// поэтому чтение завершается штатно.
    pub struct WindowsPipeClient {
        pipe_path: PathBuf,
    }

    impl WindowsPipeClient {
        pub fn new(pipe_path: impl Into<PathBuf>) -> Self {
            Self {
                pipe_path: pipe_path.into(),
            }
        }

        pub fn from_env() -> Self {
            Self::new(default_pipe_path())
        }

        pub fn pipe_path(&self) -> &Path {
            &self.pipe_path
        }

        fn connect(&self) -> Result<File, HelperClientError> {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.pipe_path)
                .map_err(|error| match error.kind() {
                    std::io::ErrorKind::NotFound => {
                        HelperClientError::NotInstalled(error.to_string())
                    }
                    _ => HelperClientError::Unavailable(error.to_string()),
                })
        }
    }

    impl HelperClient for WindowsPipeClient {
        fn status(&self) -> Result<HelperStatus, HelperClientError> {
            super::status(&mut self.connect()?)
        }

        fn start(
            &self,
            config: Value,
            rule_set: Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            super::start(&mut self.connect()?, config, rule_set, kill_switch)
        }

        fn reload_routing(&self, rule_set: Value) -> Result<HelperStatus, HelperClientError> {
            super::reload_routing(&mut self.connect()?, rule_set)
        }

        fn restart(
            &self,
            config: Value,
            rule_set: Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            super::restart(&mut self.connect()?, config, rule_set, kill_switch)
        }

        fn stop(&self) -> Result<HelperStatus, HelperClientError> {
            super::stop(&mut self.connect()?)
        }
    }
}

#[cfg(windows)]
pub use windows_pipe::WindowsPipeClient;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_is_unique() {
        assert_ne!(next_id(), next_id());
    }

    #[test]
    fn request_encodes_command_and_payload() {
        let request =
            Request::with_payload(next_id(), Command::Start, serde_json::json!({"config": {}}));
        let encoded = serde_json::to_string(&request).unwrap();
        assert!(encoded.contains(r#""command":"start""#));
    }

    #[test]
    fn io_errors_map_to_typed_variants() {
        use std::io::{Error, ErrorKind};
        assert!(matches!(
            map_io_error(Error::from(ErrorKind::NotFound)),
            HelperClientError::NotInstalled(_)
        ));
        assert!(matches!(
            map_io_error(Error::from(ErrorKind::TimedOut)),
            HelperClientError::Timeout
        ));
        assert!(matches!(
            map_io_error(Error::from(ErrorKind::PermissionDenied)),
            HelperClientError::Unavailable(_)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn pipe_path_points_to_helper_pipe() {
        assert_eq!(
            default_pipe_path(),
            PathBuf::from(r"\\.\pipe\splitroute-helper")
        );
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use crate::privileged::HelperClient;
    use splitroute_protocol::PROTOCOL_VERSION;
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

    /// Одноразовый сервер: читает запрос и отдаёт заранее заданный ответ.
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
            serde_json::json!({"protocol": PROTOCOL_VERSION, "id": "x", "ok": true, "payload": payload})
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
            serde_json::json!({"protocol": PROTOCOL_VERSION + 1, "id": "x", "ok": true, "payload": {}})
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
                "protocol": PROTOCOL_VERSION, "id": "x", "ok": false,
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
