//! Статистика трафика сессии из локального Clash API sing-box.
//!
//! Ядро публикует API только на loopback (`127.0.0.1`) со случайным `secret` на
//! каждую сессию. Читает непривилегированный процесс приложения: helper в
//! раздаче метрик не участвует (PRD §6.2). Наружу запросов нет.

use std::io::{self, Read};
use std::net::TcpListener;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;

use crate::config::{ClashApi, CLASH_API_HOST};

/// Таймаут одного опроса: loopback отвечает за миллисекунды.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(2);
/// Предел тела ответа: защита от неожиданно большого `/connections`.
pub const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Суммарные счётчики с момента запуска ядра.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrafficTotals {
    pub upload: u64,
    pub download: u64,
}

/// Порт и secret текущей сессии; secret в логи не попадает.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficEndpoint {
    api: ClashApi,
}

/// Ошибка чтения метрик. Не влияет на подключение: UI показывает «—».
#[derive(Debug, thiserror::Error)]
pub enum TrafficError {
    #[error("локальный API статистики недоступен: {0}")]
    Unavailable(String),
    #[error("локальный API статистики ответил HTTP {0}")]
    Status(u16),
    #[error("некорректный ответ статистики: {0}")]
    InvalidResponse(String),
}

impl TrafficEndpoint {
    /// Выделяет свободный loopback-порт и случайный secret.
    ///
    /// Порт занимается и сразу освобождается, чтобы его получило ядро. Гонка
    /// возможна, но безвредна: занятый порт не даст ядру запуститься, и это
    /// видно как ошибка `start`, а не молчаливая утечка.
    pub fn allocate() -> io::Result<Self> {
        let listener = TcpListener::bind((CLASH_API_HOST, 0))?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(Self::with_parts(port, random_secret()))
    }

    pub fn with_parts(port: u16, secret: impl Into<String>) -> Self {
        Self {
            api: ClashApi {
                port,
                secret: secret.into(),
            },
        }
    }

    pub fn api(&self) -> &ClashApi {
        &self.api
    }

    /// `GET /connections` с Bearer-токеном. Читаются только суммы.
    pub fn totals(&self) -> Result<TrafficTotals, TrafficError> {
        let url = format!("http://{CLASH_API_HOST}:{}/connections", self.api.port);
        let response = http_client()?
            .get(url)
            .timeout(POLL_TIMEOUT)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.api.secret),
            )
            .send()
            .map_err(|error| TrafficError::Unavailable(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(TrafficError::Status(status.as_u16()));
        }

        let mut body = String::new();
        response
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut body)
            .map_err(|error| TrafficError::Unavailable(error.to_string()))?;
        parse_connections(&body)
    }
}

#[derive(Debug, Deserialize)]
struct ConnectionsBody {
    #[serde(default, rename = "uploadTotal")]
    upload_total: u64,
    #[serde(default, rename = "downloadTotal")]
    download_total: u64,
}

/// Разбирает ответ `/connections`. Список соединений и память не нужны.
pub fn parse_connections(raw: &str) -> Result<TrafficTotals, TrafficError> {
    let body: ConnectionsBody = serde_json::from_str(raw)
        .map_err(|error| TrafficError::InvalidResponse(error.to_string()))?;
    Ok(TrafficTotals {
        upload: body.upload_total,
        download: body.download_total,
    })
}

/// Один клиент на процесс: опрос раз в секунду не должен создавать потоки.
fn http_client() -> Result<&'static reqwest::blocking::Client, TrafficError> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| reqwest::blocking::Client::builder().build().ok())
        .as_ref()
        .ok_or_else(|| TrafficError::Unavailable("не удалось создать HTTP-клиент".to_string()))
}

/// 32 hex-символа из CSPRNG — ровно как HWID, но на каждую сессию новый.
fn random_secret() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::thread;

    /// Одноразовый HTTP-сервер: читает запрос и отдаёт заданный ответ.
    fn spawn_http(
        response_head: &'static str,
        body: &'static str,
    ) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((CLASH_API_HOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let response = format!(
                    "{response_head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (port, handle)
    }

    /// Ловит Authorization в запросе, чтобы проверить передачу secret.
    fn spawn_http_capturing(response_body: &'static str) -> (u16, thread::JoinHandle<String>) {
        let listener = TcpListener::bind((CLASH_API_HOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let mut request = String::new();
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    request.push_str(&line);
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                    response_body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
            request
        });
        (port, handle)
    }

    #[test]
    fn parses_totals_and_ignores_extra_fields() {
        let raw = r#"{"connections":[],"downloadTotal":200204,"memory":4792320,"uploadTotal":89}"#;
        let totals = parse_connections(raw).unwrap();
        assert_eq!(
            totals,
            TrafficTotals {
                upload: 89,
                download: 200204
            }
        );
    }

    #[test]
    fn missing_totals_default_to_zero() {
        assert_eq!(parse_connections("{}").unwrap(), TrafficTotals::default());
    }

    #[test]
    fn invalid_body_is_error() {
        assert!(matches!(
            parse_connections("not json"),
            Err(TrafficError::InvalidResponse(_))
        ));
    }

    #[test]
    fn reads_totals_from_loopback_api_with_secret() {
        crate::init_tls();
        let body = r#"{"connections":[],"downloadTotal":4096,"memory":1,"uploadTotal":1024}"#;
        let (port, handle) = spawn_http_capturing(body);
        let endpoint = TrafficEndpoint::with_parts(port, "test-secret");
        let totals = endpoint.totals().unwrap();
        assert_eq!(
            totals,
            TrafficTotals {
                upload: 1024,
                download: 4096
            }
        );
        let request = handle.join().unwrap();
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-secret"),
            "secret должен передаваться в заголовке: {request}"
        );
        assert!(request.starts_with("GET /connections"));
    }

    #[test]
    fn non_success_status_is_error() {
        crate::init_tls();
        let (port, handle) = spawn_http("HTTP/1.1 401 Unauthorized", "");
        let endpoint = TrafficEndpoint::with_parts(port, "wrong");
        assert!(matches!(endpoint.totals(), Err(TrafficError::Status(401))));
        handle.join().unwrap();
    }

    #[test]
    fn missing_api_is_unavailable() {
        crate::init_tls();
        // Порт закрыт: соединение отклоняется, ошибка не паникует.
        let listener = TcpListener::bind((CLASH_API_HOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let endpoint = TrafficEndpoint::with_parts(port, "secret");
        assert!(matches!(
            endpoint.totals(),
            Err(TrafficError::Unavailable(_))
        ));
    }

    #[test]
    fn allocate_uses_loopback_and_random_secret() {
        let first = TrafficEndpoint::allocate().unwrap();
        let second = TrafficEndpoint::allocate().unwrap();
        assert_ne!(first.api().secret, second.api().secret);
        assert_eq!(first.api().secret.len(), 32);
        assert!(first
            .api()
            .secret
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first.api().port, 0);
        assert_ne!(second.api().port, 0);
    }

    /// Ответ обрывается на середине: ошибка, а не паника и не зависание.
    #[test]
    fn truncated_body_is_error() {
        crate::init_tls();
        let listener = TcpListener::bind((CLASH_API_HOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\nConnection: close\r\n\r\n{",
                );
            }
        });
        let endpoint = TrafficEndpoint::with_parts(port, "secret");
        assert!(matches!(
            endpoint.totals(),
            Err(TrafficError::Unavailable(_)) | Err(TrafficError::InvalidResponse(_))
        ));
        handle.join().unwrap();
    }
}
