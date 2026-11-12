//! Загрузка подписки по HTTP с типизированными ошибками.
//!
//! Секреты (URL, HWID) не попадают в сообщения об ошибках и логи. Ошибки сети
//! намеренно не содержат URL: `reqwest` подставляет его в `Display`.

use std::time::Duration;

use super::{parse_subscription_report, Subscription, SubscriptionError};

/// Параметры запроса к серверу подписки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: String,
    pub hwid: String,
    pub user_agent: String,
}

/// Типизированная ошибка загрузки/разбора подписки.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("URL подписки не задан")]
    MissingUrl,
    #[error("ошибка сети: {0}")]
    Network(String),
    #[error("сервер отклонил HWID (401)")]
    Unauthorized,
    #[error("доступ запрещён (403)")]
    Forbidden,
    #[error("подписка не найдена (404)")]
    NotFound,
    #[error("сервер вернул HTTP {0}")]
    Status(u16),
    #[error("невалидный JSON: {0}")]
    InvalidJson(String),
    #[error("подписка пуста")]
    Empty,
    #[error("нет поддерживаемых профилей: пропущено {skipped} из {total}")]
    NoProfiles { total: usize, skipped: usize },
}

impl FetchError {
    /// Стабильный машинно-читаемый код для UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingUrl => "MISSING_URL",
            Self::Network(_) => "NETWORK",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::Status(_) => "HTTP_ERROR",
            Self::InvalidJson(_) => "INVALID_JSON",
            Self::Empty => "EMPTY",
            Self::NoProfiles { .. } => "NO_PROFILES",
        }
    }
}

impl From<SubscriptionError> for FetchError {
    fn from(error: SubscriptionError) -> Self {
        match error {
            SubscriptionError::InvalidJson(message) => Self::InvalidJson(message),
            SubscriptionError::NotAnArray => {
                Self::InvalidJson("ожидался JSON-массив профилей".to_string())
            }
            SubscriptionError::Empty => Self::Empty,
            SubscriptionError::NoProfiles { total, skipped } => Self::NoProfiles { total, skipped },
        }
    }
}

/// Источник тела подписки. Реализация подменяется в тестах.
pub trait SubscriptionFetcher: Send + Sync {
    fn fetch(&self, request: &FetchRequest) -> Result<String, FetchError>;
}

/// HTTP-загрузчик на `reqwest` + rustls.
pub struct ReqwestFetcher {
    timeout: Duration,
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(20),
        }
    }
}

impl Default for ReqwestFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionFetcher for ReqwestFetcher {
    fn fetch(&self, request: &FetchRequest) -> Result<String, FetchError> {
        if request.url.trim().is_empty() {
            return Err(FetchError::MissingUrl);
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| FetchError::Network(network_message(&error)))?;

        let response = client
            .get(request.url.trim())
            .header("X-HWID", &request.hwid)
            .header("Accept", "application/json")
            .header(
                reqwest::header::USER_AGENT,
                if request.user_agent.trim().is_empty() {
                    "SplitRoute/0.1.0"
                } else {
                    request.user_agent.trim()
                },
            )
            .send()
            .map_err(|error| FetchError::Network(network_message(&error)))?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(status_error(status));
        }

        response
            .text()
            .map_err(|error| FetchError::Network(network_message(&error)))
    }
}

/// Загружает тело и разбирает его в профили.
pub fn fetch_and_parse(
    fetcher: &dyn SubscriptionFetcher,
    request: &FetchRequest,
) -> Result<Subscription, FetchError> {
    let body = fetcher.fetch(request)?;
    Ok(parse_subscription_report(&body)?)
}

fn status_error(status: u16) -> FetchError {
    match status {
        401 => FetchError::Unauthorized,
        403 => FetchError::Forbidden,
        404 => FetchError::NotFound,
        other => FetchError::Status(other),
    }
}

/// Сообщение об ошибке сети без URL и заголовков.
fn network_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "истёк таймаут запроса".to_string()
    } else if error.is_connect() {
        "не удалось установить соединение".to_string()
    } else {
        "запрос не выполнен".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    struct MockFetcher {
        body: Result<String, FetchError>,
    }

    impl SubscriptionFetcher for MockFetcher {
        fn fetch(&self, _request: &FetchRequest) -> Result<String, FetchError> {
            self.body.clone()
        }
    }

    fn request() -> FetchRequest {
        FetchRequest {
            url: "https://example.invalid/sub".to_string(),
            hwid: "0123456789abcdef".to_string(),
            user_agent: "test".to_string(),
        }
    }

    fn fixture() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../spike/fixtures/subscription.json"
        ))
        .expect("фикстура подписки")
    }

    #[test]
    fn parses_fixture_through_fetcher() {
        let fetcher = MockFetcher {
            body: Ok(fixture()),
        };
        let subscription = fetch_and_parse(&fetcher, &request()).unwrap();
        assert_eq!(subscription.profiles.len(), 2);
        assert!(subscription.skipped.is_empty());
    }

    #[test]
    fn invalid_json_is_typed() {
        let fetcher = MockFetcher {
            body: Ok("{oops".to_string()),
        };
        let error = fetch_and_parse(&fetcher, &request()).unwrap_err();
        assert_eq!(error.code(), "INVALID_JSON");
    }

    #[test]
    fn empty_array_is_typed() {
        let fetcher = MockFetcher {
            body: Ok("[]".to_string()),
        };
        let error = fetch_and_parse(&fetcher, &request()).unwrap_err();
        assert_eq!(error, FetchError::Empty);
    }

    #[test]
    fn no_supported_profiles_is_typed() {
        let fetcher = MockFetcher {
            body: Ok(r#"[{"remarks":"x","outbounds":[]}]"#.to_string()),
        };
        let error = fetch_and_parse(&fetcher, &request()).unwrap_err();
        assert_eq!(
            error,
            FetchError::NoProfiles {
                total: 1,
                skipped: 1
            }
        );
    }

    #[test]
    fn network_error_is_passed_through_without_url() {
        let fetcher = MockFetcher {
            body: Err(FetchError::Network("запрос не выполнен".to_string())),
        };
        let error = fetch_and_parse(&fetcher, &request()).unwrap_err();
        assert_eq!(error.code(), "NETWORK");
        assert!(!error.to_string().contains("example.invalid"));
    }

    #[test]
    fn status_codes_are_mapped() {
        assert_eq!(status_error(401), FetchError::Unauthorized);
        assert_eq!(status_error(403), FetchError::Forbidden);
        assert_eq!(status_error(404), FetchError::NotFound);
        assert_eq!(status_error(500), FetchError::Status(500));
    }

    #[test]
    fn empty_url_is_missing() {
        let fetcher = ReqwestFetcher::new();
        let mut broken = request();
        broken.url = "  ".to_string();
        assert_eq!(fetcher.fetch(&broken).unwrap_err(), FetchError::MissingUrl);
    }

    /// Одноразовый HTTP-сервер: принимает один запрос и отдаёт заданный ответ.
    fn spawn_http_server(status_line: &str, body: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status_line = status_line.to_string();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://{address}/subscription.json"), handle)
    }

    fn init_tls() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    #[test]
    fn http_statuses_are_mapped_end_to_end() {
        init_tls();
        for (status, expected) in [
            ("401 Unauthorized", FetchError::Unauthorized),
            ("403 Forbidden", FetchError::Forbidden),
            ("404 Not Found", FetchError::NotFound),
            ("500 Internal Server Error", FetchError::Status(500)),
        ] {
            let (url, handle) = spawn_http_server(status, String::new());
            let mut target = request();
            target.url = url;
            assert_eq!(
                ReqwestFetcher::new().fetch(&target).unwrap_err(),
                expected,
                "статус {status}"
            );
            handle.join().unwrap();
        }
    }

    #[test]
    fn http_fixture_is_parsed_end_to_end() {
        init_tls();
        let (url, handle) = spawn_http_server("200 OK", fixture());
        let mut target = request();
        target.url = url;
        let subscription = fetch_and_parse(&ReqwestFetcher::new(), &target).unwrap();
        assert_eq!(subscription.profiles.len(), 2);
        handle.join().unwrap();
    }

    #[test]
    fn http_errors_from_body_are_typed() {
        init_tls();
        for (body, code) in [
            ("{oops".to_string(), "INVALID_JSON"),
            ("[]".to_string(), "EMPTY"),
            (
                r#"[{"remarks":"x","outbounds":[]}]"#.to_string(),
                "NO_PROFILES",
            ),
        ] {
            let (url, handle) = spawn_http_server("200 OK", body);
            let mut target = request();
            target.url = url;
            let error = fetch_and_parse(&ReqwestFetcher::new(), &target).unwrap_err();
            assert_eq!(error.code(), code);
            handle.join().unwrap();
        }
    }
}
