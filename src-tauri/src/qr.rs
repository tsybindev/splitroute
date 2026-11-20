//! Декодирование QR-кодов для импорта подписки с картинки (PRD §6.1).
//!
//! Пользователь может показать QR со ссылкой на подписку: скриншотом или
//! файлом. Здесь только механика — декодировать все коды с изображения и
//! отнести результат к одному из известных форматов (`url`, `happ`,
//! `node_links`). Ссылки на подписку — секрет, поэтому декодированный текст
//! нигде не логируется.

use serde::Serialize;

use crate::subscription::base64;

/// Префикс ссылки-узла, по которой распознаётся список локаций.
const VLESS_PREFIX: &str = "vless://";

/// Значения поля `kind` в [`QrDecodeResponse`].
const KIND_URL: &str = "url";
const KIND_HAPP: &str = "happ";
const KIND_NODE_LINKS: &str = "node_links";
const KIND_UNKNOWN: &str = "unknown";

/// Единая ошибка декодирования: не удалось прочитать изображение либо на нём
/// не нашлось ни одного QR-кода. Для UI обе причины неотличимы — ответ один.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct QrError(pub String);

/// Тип содержимого QR-кода после классификации одного текста.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrPayload {
    /// Прямая ссылка на подписку.
    Url(String),
    /// Happ-ссылка, внутри которой спрятана ссылка на подписку.
    HappUrl(String),
    /// Текст содержит `vless://`-ссылки (список узлов).
    NodeLinks,
    /// Ничего узнаваемого.
    Unknown,
}

/// Результат для UI: тип содержимого и URL для дальнейшей загрузки. `url`
/// заполнен только для `kind` `url` и `happ` — при `node_links` и `unknown`
/// он `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QrDecodeResponse {
    pub kind: String,
    pub url: Option<String>,
}

/// Декодирует base64-байты изображения, пришедшие от вебвью (`btoa` даёт
/// standard-алфавит с паддингом). `None` — тело не является base64.
pub fn decode_base64_payload(input: &str) -> Option<Vec<u8>> {
    base64::decode(input)
}

/// Декодирует все QR-коды с изображения (PNG или JPEG).
///
/// Возвращает тексты найденных кодов в порядке обнаружения. Ошибка — и когда
/// байты не читаются как изображение, и когда кодов на нём нет: для UI это
/// одна и та же «нет QR» ситуация.
pub fn decode_image(bytes: &[u8]) -> Result<Vec<String>, QrError> {
    let image = image::load_from_memory(bytes).map_err(|_| {
        QrError("Не удалось прочитать изображение. Поддерживаются PNG и JPEG.".to_string())
    })?;
    let luma = image.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(luma);
    let texts: Vec<String> = prepared
        .detect_grids()
        .iter()
        .filter_map(|grid| grid.decode().ok().map(|(_, content)| content))
        .collect();
    if texts.is_empty() {
        Err(QrError("QR-код на изображении не найден.".to_string()))
    } else {
        Ok(texts)
    }
}

/// Относит декодированные тексты к формату импорта. Порядок текстов — как их
/// вернул декодер (слева направо, сверху вниз): первый подходящий `url`/`happ`
/// выигрывает; если прямых ссылок нет, но встречается `vless://` — список
/// узлов; иначе `unknown`.
pub fn route_payload(texts: &[&str]) -> QrDecodeResponse {
    let mut node_links = false;
    for text in texts {
        match classify_text(text) {
            QrPayload::Url(url) => {
                return QrDecodeResponse {
                    kind: KIND_URL.to_string(),
                    url: Some(url),
                }
            }
            QrPayload::HappUrl(url) => {
                return QrDecodeResponse {
                    kind: KIND_HAPP.to_string(),
                    url: Some(url),
                }
            }
            QrPayload::NodeLinks => node_links = true,
            QrPayload::Unknown => {}
        }
    }
    if node_links {
        QrDecodeResponse {
            kind: KIND_NODE_LINKS.to_string(),
            url: None,
        }
    } else {
        QrDecodeResponse {
            kind: KIND_UNKNOWN.to_string(),
            url: None,
        }
    }
}

/// Классифицирует один текст QR-кода.
fn classify_text(text: &str) -> QrPayload {
    let trimmed = text.trim();
    if is_http_url(trimmed) && has_host(trimmed) {
        return QrPayload::Url(trimmed.to_string());
    }
    if let Some(value) = happ_value(trimmed) {
        return happ_payload(&value);
    }
    if trimmed.contains(VLESS_PREFIX) {
        return QrPayload::NodeLinks;
    }
    QrPayload::Unknown
}

/// `true` — текст начинается с `http://` или `https://`.
fn is_http_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

/// Выделяет authority URL и проверяет, что хост в нём непустой (после
/// необязательной `user-info`). `http://` без хоста ссылкой не считается.
fn has_host(url: &str) -> bool {
    let Some((_, authority)) = url.split_once("://") else {
        return false;
    };
    let authority = authority.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    !host.is_empty()
}

/// Извлекает значение из happ-ссылки, если текст действительно начинается с
/// неё. Поддерживаются обе формы: `happ://add?url=<значение>` (значение до
/// следующего `&`) и `happ://add/<значение>` (до `?` или конца текста;
/// в значении встречается base64, поэтому `/` внутри не рвёт его).
fn happ_value(trimmed: &str) -> Option<String> {
    const QUERY_PREFIX: &str = "happ://add?url=";
    const PATH_PREFIX: &str = "happ://add/";

    if let Some(value) = trimmed.strip_prefix(QUERY_PREFIX) {
        return Some(value.split('&').next().unwrap_or("").to_string());
    }
    trimmed
        .strip_prefix(PATH_PREFIX)
        .map(|value| value.split(['?', '#']).next().unwrap_or("").to_string())
}

/// Расшифровывает значение happ-ссылки: сначала base64 (стандартный или
/// URL-safe, с паддингом или без — терпимый декодер подписок), если из него
/// не получился URL — процентное декодирование.
fn happ_payload(value: &str) -> QrPayload {
    if let Some(decoded) = base64::decode(value) {
        if let Ok(text) = String::from_utf8(decoded) {
            let trimmed = text.trim();
            if is_http_url(trimmed) {
                return QrPayload::HappUrl(trimmed.to_string());
            }
        }
    }
    let decoded = percent_decode(value);
    let trimmed = decoded.trim();
    if is_http_url(trimmed) {
        QrPayload::HappUrl(trimmed.to_string())
    } else {
        QrPayload::Unknown
    }
}

/// Процентное декодирование `%XX`; некорректные последовательности остаются
/// как есть, `+` не превращается в пробел (в значениях встречается base64).
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use qrcode::{Color, QrCode};

    use super::*;

    /// `happ://add?url=<значение>` с URL-safe base64 без паддинга.
    #[test]
    fn routes_happ_query_with_urlsafe_base64() {
        // base64("https://example.invalid/sub") без паддинга.
        let text = "happ://add?url=aHR0cHM6Ly9leGFtcGxlLmludmFsaWQvc3Vi";
        let response = route_payload(&[text]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "happ".to_string(),
                url: Some("https://example.invalid/sub".to_string()),
            }
        );
    }

    /// Стандартный base64 с паддингом (btoa из вебвью) тоже читается.
    #[test]
    fn routes_happ_query_with_standard_base64() {
        // base64("https://example.invalid/sub?a=1&b=2") с паддингом.
        let text = "happ://add?url=aHR0cHM6Ly9leGFtcGxlLmludmFsaWQvc3ViP2E9MSZiPTI=";
        let response = route_payload(&[text]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "happ".to_string(),
                url: Some("https://example.invalid/sub?a=1&b=2".to_string()),
            }
        );
    }

    /// Значение может быть процентно-закодированным URL без base64.
    #[test]
    fn routes_happ_query_with_percent_encoded_url() {
        let text = "happ://add?url=https%3A%2F%2Fexample.invalid%2Fsub";
        let response = route_payload(&[text]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "happ".to_string(),
                url: Some("https://example.invalid/sub".to_string()),
            }
        );
    }

    /// Вторая форма: значение в первом сегменте пути.
    #[test]
    fn routes_happ_path_with_base64() {
        let text = "happ://add/aHR0cHM6Ly9leGFtcGxlLmludmFsaWQvc3Vi";
        let response = route_payload(&[text]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "happ".to_string(),
                url: Some("https://example.invalid/sub".to_string()),
            }
        );
    }

    /// Значение, из которого не получается URL, — unknown.
    #[test]
    fn happ_with_garbage_value_is_unknown() {
        // base64("not-a-subscription-url") — не http(s).
        let text = "happ://add?url=bm90LWEtc3Vic2NyaXB0aW9uLXVybA==";
        let response = route_payload(&[text]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "unknown".to_string(),
                url: None,
            }
        );
    }

    /// Прямые http/https-ссылки распознаются с обрезкой пробелов.
    #[test]
    fn routes_http_and_https_urls() {
        let response = route_payload(&["  http://example.invalid/sub\n"]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "url".to_string(),
                url: Some("http://example.invalid/sub".to_string()),
            }
        );
        let response = route_payload(&["https://example.invalid/sub"]);
        assert_eq!(response.kind, "url");
        assert_eq!(response.url.as_deref(), Some("https://example.invalid/sub"));
    }

    /// `http://` без хоста ссылкой не считается.
    #[test]
    fn url_without_host_is_unknown() {
        for text in ["http://", "https://"] {
            let response = route_payload(&[text]);
            assert_eq!(
                response,
                QrDecodeResponse {
                    kind: "unknown".to_string(),
                    url: None,
                },
                "{text} без хоста"
            );
        }
    }

    /// Текст с `vless://`-ссылкой — список узлов, без URL.
    #[test]
    fn vless_text_routes_to_node_links() {
        let text =
            "vless://00000000-0000-0000-0000-000000000000@example.invalid:443?encryption=none";
        let response = route_payload(&[text]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "node_links".to_string(),
                url: None,
            }
        );
    }

    /// Прямая ссылка побеждает, даже если `vless://` встретился раньше.
    #[test]
    fn first_url_wins_over_node_links() {
        let vless = "vless://00000000-0000-0000-0000-000000000000@example.invalid:443";
        let response = route_payload(&[vless, "  https://example.invalid/sub  ", "мусор"]);
        assert_eq!(response.kind, "url");
        assert_eq!(response.url.as_deref(), Some("https://example.invalid/sub"));
    }

    /// `happ`-ссылка тоже побеждает `vless://` из более раннего текста.
    #[test]
    fn happ_wins_over_earlier_node_links() {
        let vless = "vless://00000000-0000-0000-0000-000000000000@example.invalid:443";
        let happ = "happ://add?url=https%3A%2F%2Fexample.invalid%2Fsub";
        let response = route_payload(&[vless, happ]);
        assert_eq!(response.kind, "happ");
        assert_eq!(response.url.as_deref(), Some("https://example.invalid/sub"));
    }

    /// Список узлов находится даже после неопознанного текста.
    #[test]
    fn node_links_found_after_unknown_text() {
        let vless = "vless://00000000-0000-0000-0000-000000000000@example.invalid:443";
        let response = route_payload(&["случайный текст", vless]);
        assert_eq!(response.kind, "node_links");
        assert_eq!(response.url, None);
    }

    /// Ничего узнаваемого или пустой список — unknown.
    #[test]
    fn unknown_for_garbage_and_empty_input() {
        let response = route_payload(&["просто текст", "{\"json\": true}"]);
        assert_eq!(
            response,
            QrDecodeResponse {
                kind: "unknown".to_string(),
                url: None,
            }
        );
        let response = route_payload(&[]);
        assert_eq!(response.kind, "unknown");
        assert_eq!(response.url, None);
    }

    /// Процентное декодирование: валидные `%XX` раскрываются, битые
    /// последовательности и `+` остаются как есть (как в vless.rs).
    #[test]
    fn percent_decoding_keeps_plus_and_invalid_sequences() {
        assert_eq!(
            percent_decode("https%3A%2F%2Fexample.invalid"),
            "https://example.invalid"
        );
        assert_eq!(percent_decode("a%20b%2Fc+d"), "a b/c+d");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
        assert_eq!(percent_decode("100%"), "100%");
    }

    // ---- decode_image: тестовые PNG генерируются крейтом `qrcode` ----

    const SCALE: u32 = 6;
    /// Стандартная тихая зона QR — 4 модуля с каждой стороны.
    const QUIET_MODULES: u32 = 4;

    /// Рисует QR-код в лuma-буфер: тёмные модули из `qrcode`, вокруг — тихая
    /// зона. PNG кодирует крейт `image` из обычных зависимостей.
    fn qr_gray(text: &str) -> image::GrayImage {
        let code = QrCode::new(text).unwrap();
        let colors = code.to_colors();
        let size = code.width() as u32;
        let full = size + 2 * QUIET_MODULES;
        let width = full * SCALE;
        let mut buffer = vec![255u8; (width * width) as usize];
        for y in 0..size {
            for x in 0..size {
                if colors[(y * size + x) as usize] != Color::Dark {
                    continue;
                }
                let origin = (x + QUIET_MODULES) * SCALE;
                let origin_y = (y + QUIET_MODULES) * SCALE;
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let index = (origin_y + dy) * width + origin + dx;
                        buffer[index as usize] = 0;
                    }
                }
            }
        }
        image::GrayImage::from_raw(width, width, buffer).unwrap()
    }

    /// Кодирует лuma-буфер в PNG-байты — как раз то, что приходит в
    /// [`decode_image`] от команды.
    fn gray_to_png(image: image::GrayImage) -> Vec<u8> {
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    }

    #[test]
    fn decodes_short_url_png() {
        let png = gray_to_png(qr_gray("https://example.invalid/sub"));
        assert_eq!(
            decode_image(&png).unwrap(),
            vec!["https://example.invalid/sub".to_string()]
        );
    }

    /// Длинный URL (~200 символов): большой QR с большим числом модулей.
    #[test]
    fn decodes_long_url_png() {
        let url = format!("https://example.invalid/sub?token={}", "a".repeat(160));
        let png = gray_to_png(qr_gray(&url));
        assert_eq!(decode_image(&png).unwrap(), vec![url]);
    }

    /// Ссылка-узел с картинки распознаётся как текст.
    #[test]
    fn decodes_vless_link_png() {
        let vless =
            "vless://00000000-0000-0000-0000-000000000000@example.invalid:443?encryption=none";
        let png = gray_to_png(qr_gray(vless));
        assert_eq!(decode_image(&png).unwrap(), vec![vless.to_string()]);
    }

    /// На картинке два QR рядом — декодер возвращает оба текста.
    #[test]
    fn decodes_two_codes_in_one_image() {
        let first = qr_gray("https://first.example.invalid/a");
        let second = qr_gray("https://second.example.invalid/b");
        // Зазор в 4 модуля между кодами, поверх их собственных тихих зон.
        let gap = 4 * SCALE;
        let width = first.width() + gap + second.width();
        let height = first.height().max(second.height());
        let mut composite = image::GrayImage::from_pixel(width, height, image::Luma([255u8]));
        image::imageops::overlay(&mut composite, &first, 0, 0);
        image::imageops::overlay(&mut composite, &second, (first.width() + gap).into(), 0);

        let texts = decode_image(&gray_to_png(composite)).unwrap();
        assert!(texts.contains(&"https://first.example.invalid/a".to_string()));
        assert!(texts.contains(&"https://second.example.invalid/b".to_string()));
    }

    /// Байты, не являющиеся изображением, — единая ошибка декодирования.
    #[test]
    fn rejects_non_image_bytes() {
        let error = decode_image(b"this is not an image").unwrap_err();
        assert!(error.0.contains("изображение"));
    }

    /// Изображение без QR-кода — та же ошибка: для UI разницы нет.
    #[test]
    fn rejects_image_without_qr() {
        let blank = image::GrayImage::from_pixel(240, 240, image::Luma([255u8]));
        let error = decode_image(&gray_to_png(blank)).unwrap_err();
        assert!(error.0.contains("не найден"));
    }

    /// Связка декодера и роутинга: QR с happ-ссылкой превращается в ответ
    /// команды `happ`.
    #[test]
    fn decoded_happ_routes_to_happ_kind() {
        let png = gray_to_png(qr_gray(
            "happ://add?url=https%3A%2F%2Fexample.invalid%2Fsub",
        ));
        let texts = decode_image(&png).unwrap();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let response = route_payload(&refs);
        assert_eq!(response.kind, "happ");
        assert_eq!(response.url.as_deref(), Some("https://example.invalid/sub"));
    }
}
