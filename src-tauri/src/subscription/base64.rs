//! Base64 для тел подписок.
//!
//! Провайдеры отдают список `vless://`-ссылок как base64: встречаются
//! стандартный и URL-safe алфавиты, паддинг и его отсутствие, переносы строк.
//! Декодер намеренно терпимый: задача — прочитать тело подписки, а не
//! валидировать чужой контейнер.

/// Декодирует base64 (standard или URL-safe, с паддингом или без).
/// `None` — тело не является base64.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut padding = false;
    let mut symbols = 0usize;

    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            // `=` встречается только в конце; данные после него — уже не base64.
            b'=' => {
                padding = true;
                continue;
            }
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        if padding {
            return None;
        }
        symbols += 1;
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }

    if symbols == 0 || output.is_empty() {
        None
    } else {
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_standard_alphabet() {
        assert_eq!(decode("Zm9vYmFy").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(decode("Zm9vYmE=").as_deref(), Some(&b"fooba"[..]));
        assert_eq!(decode("Zg==").as_deref(), Some(&b"f"[..]));
    }

    #[test]
    fn decodes_url_safe_alphabet() {
        assert_eq!(decode("//79").as_deref(), Some(&[0xff, 0xfe, 0xfd][..]));
        assert_eq!(decode("__79").as_deref(), Some(&[0xff, 0xfe, 0xfd][..]));
    }

    #[test]
    fn ignores_whitespace_and_missing_padding() {
        assert_eq!(decode("Zm9v\nYmFy\n").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(decode("Zg").as_deref(), Some(&b"f"[..]));
        assert_eq!(
            decode("dmxlc3M6Ly8").as_deref(),
            Some(&b"vless://"[..]),
            "тело подписки может быть без паддинга"
        );
    }

    #[test]
    fn rejects_non_base64() {
        assert!(decode("vless://example").is_none());
        assert!(decode("{\"json\": true}").is_none());
        assert!(decode("").is_none());
        assert!(decode("Zg==Zg").is_none(), "данные после паддинга");
    }
}
