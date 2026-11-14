//! Разбор списка `vless://`-ссылок в модели профилей.
//!
//! Такой формат провайдеры отдают для не-Happ User-Agent (в base64 или
//! открытым текстом). Поддерживается обязательный набор MVP: VLESS + Reality
//! или TLS + TCP. Остальные транспорты и security пропускаются с причиной,
//! как и в JSON-парсере.

use std::collections::HashMap;

use splitroute_protocol::{Profile, Protocol, TlsConfig, TlsKind, Transport};

use super::{is_uuid, profile_id, SkippedProfile, Subscription};

/// Есть ли в тексте хотя бы одна vless-ссылка.
pub fn contains_links(text: &str) -> bool {
    text.lines().any(|line| is_link(line.trim()))
}

/// Разбирает список ссылок: по одной на строку, пустые и `#`-комментарии
/// игнорируются.
pub fn parse_links(text: &str) -> Subscription {
    let mut profiles = Vec::new();
    let mut skipped = Vec::new();
    let mut index = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        index += 1;
        if !is_link(line) {
            skipped.push(SkippedProfile {
                index,
                name: String::new(),
                reason: "не vless-ссылка".to_string(),
            });
            continue;
        }
        match parse_link(line) {
            Ok(profile) => profiles.push(profile),
            Err(reason) => skipped.push(SkippedProfile {
                index,
                name: fragment_name(line),
                reason,
            }),
        }
    }

    Subscription { profiles, skipped }
}

/// Разбирает одну ссылку вида
/// `vless://uuid@host:port?security=reality&...#name`.
pub fn parse_link(link: &str) -> Result<Profile, String> {
    let rest = strip_scheme(link).ok_or_else(|| "не vless-ссылка".to_string())?;
    let (rest, fragment) = split_once_optional(rest, '#');
    let (authority, query) = split_once_optional(rest, '?');

    let (user, host_port) = authority
        .split_once('@')
        .ok_or_else(|| "нет uuid@host:port".to_string())?;
    let uuid = user.trim();
    if !is_uuid(uuid) {
        return Err("некорректный uuid".to_string());
    }

    let (server, port) = split_host_port(host_port)?;
    let params = parse_query(query);
    let tls = parse_tls(&params, &server)?;
    let network = params
        .get("type")
        .or_else(|| params.get("network"))
        .map(String::as_str)
        .unwrap_or("tcp");
    let transport = match network {
        "tcp" | "" => Transport::Tcp,
        other => return Err(format!("network={other} не поддерживается")),
    };

    let name = fragment
        .map(percent_decode)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Профиль".to_string());
    let flow = params
        .get("flow")
        .filter(|value| !value.is_empty())
        .cloned();

    Ok(Profile {
        id: profile_id(&name, &server, port, uuid),
        name,
        protocol: Protocol::Vless,
        server,
        port,
        uuid: uuid.to_string(),
        flow,
        tls,
        transport,
        // Ссылка не содержит полного Xray-конфига, поэтому raw не заполняется.
        raw: None,
    })
}

fn is_link(value: &str) -> bool {
    value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("vless://"))
}

fn strip_scheme(value: &str) -> Option<&str> {
    if is_link(value) {
        Some(&value[8..])
    } else {
        None
    }
}

fn split_once_optional(value: &str, separator: char) -> (&str, Option<&str>) {
    match value.split_once(separator) {
        Some((head, tail)) => (head, Some(tail)),
        None => (value, None),
    }
}

/// Имя из фрагмента ссылки — для отчёта о пропущенной записи.
fn fragment_name(link: &str) -> String {
    link.rsplit_once('#')
        .map(|(_, fragment)| percent_decode(fragment))
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn split_host_port(value: &str) -> Result<(String, u16), String> {
    let value = value.trim();
    let (host, port) = if let Some(rest) = value.strip_prefix('[') {
        // IPv6 в ссылке записывается как [::1]:443.
        let (host, tail) = rest
            .split_once(']')
            .ok_or_else(|| "некорректный IPv6-адрес".to_string())?;
        let port = tail
            .strip_prefix(':')
            .ok_or_else(|| "не указан порт".to_string())?;
        (host, port)
    } else {
        value
            .rsplit_once(':')
            .ok_or_else(|| "не указан порт".to_string())?
    };
    if host.is_empty() {
        return Err("пустой адрес".to_string());
    }
    let port: u16 = port.parse().map_err(|_| "некорректный порт".to_string())?;
    if port == 0 {
        return Err("некорректный порт".to_string());
    }
    Ok((host.to_string(), port))
}

fn parse_query(query: Option<&str>) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query.unwrap_or_default().split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(percent_decode(key), percent_decode(value));
    }
    params
}

fn parse_tls(params: &HashMap<String, String>, server: &str) -> Result<TlsConfig, String> {
    let security = params.get("security").map(String::as_str).unwrap_or("none");
    let server_name = params
        .get("sni")
        .or_else(|| params.get("serverName"))
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| server.to_string());
    let fingerprint = params
        .get("fp")
        .or_else(|| params.get("fingerprint"))
        .filter(|value| !value.is_empty())
        .cloned();
    let alpn = params
        .get("alpn")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    match security {
        "reality" => {
            let public_key = params
                .get("pbk")
                .or_else(|| params.get("publicKey"))
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "в reality-ссылке нет pbk".to_string())?;
            Ok(TlsConfig {
                kind: TlsKind::Reality,
                server_name,
                public_key: Some(public_key.clone()),
                short_id: params
                    .get("sid")
                    .or_else(|| params.get("shortId"))
                    .filter(|value| !value.is_empty())
                    .cloned(),
                fingerprint,
                alpn,
            })
        }
        "tls" => Ok(TlsConfig {
            kind: TlsKind::Tls,
            server_name,
            public_key: None,
            short_id: None,
            fingerprint,
            alpn,
        }),
        other => Err(format!("security={other} не поддерживается")),
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
    use super::*;

    const UUID: &str = "d811a5ee-1d55-4dca-bd7f-f68b768a2241";

    fn link(extra: &str) -> String {
        format!(
            "vless://{UUID}@example.com:443?encryption=none&security=reality&sni=example.com&fp=chrome&pbk=public-key&sid=00e5e640a81a9ac1&type=tcp{extra}"
        )
    }

    #[test]
    fn parses_reality_link() {
        let profile = parse_link(&format!("{}#Test%20Location", link(""))).unwrap();
        assert_eq!(profile.name, "Test Location");
        assert_eq!(profile.protocol, Protocol::Vless);
        assert_eq!(profile.server, "example.com");
        assert_eq!(profile.port, 443);
        assert_eq!(profile.uuid, UUID);
        assert_eq!(profile.tls.kind, TlsKind::Reality);
        assert_eq!(profile.tls.public_key.as_deref(), Some("public-key"));
        assert_eq!(profile.tls.short_id.as_deref(), Some("00e5e640a81a9ac1"));
        assert_eq!(profile.tls.fingerprint.as_deref(), Some("chrome"));
        assert_eq!(profile.tls.server_name, "example.com");
        assert_eq!(profile.transport, Transport::Tcp);
        assert!(profile.flow.is_none());
        assert!(profile.raw.is_none());
    }

    #[test]
    fn parses_tls_link_with_flow_and_alpn() {
        let link = format!(
            "vless://{UUID}@[2001:db8::1]:8443?security=tls&sni=cdn.example&alpn=h2,http/1.1&flow=xtls-rprx-vision&type=tcp#TLS"
        );
        let profile = parse_link(&link).unwrap();
        assert_eq!(profile.server, "2001:db8::1");
        assert_eq!(profile.port, 8443);
        assert_eq!(profile.tls.kind, TlsKind::Tls);
        assert_eq!(profile.tls.server_name, "cdn.example");
        assert_eq!(profile.tls.alpn, vec!["h2", "http/1.1"]);
        assert_eq!(profile.flow.as_deref(), Some("xtls-rprx-vision"));
    }

    #[test]
    fn skips_unsupported_network_and_security() {
        let ws = format!("vless://{UUID}@example.com:443?security=tls&type=ws#WS");
        assert!(parse_link(&ws).unwrap_err().contains("network=ws"));

        let none = format!("vless://{UUID}@example.com:443?security=none&type=tcp#Plain");
        assert!(parse_link(&none).unwrap_err().contains("security=none"));
    }

    #[test]
    fn requires_reality_public_key() {
        let broken = format!("vless://{UUID}@example.com:443?security=reality&type=tcp#Broken");
        assert!(parse_link(&broken).unwrap_err().contains("pbk"));
    }

    #[test]
    fn rejects_malformed_links() {
        assert!(parse_link("https://example.com").is_err());
        assert!(parse_link("vless://not-a-uuid@example.com:443").is_err());
        assert!(parse_link(&format!("vless://{UUID}@example.com?security=tls")).is_err());
        assert!(parse_link(&format!("vless://{UUID}@example.com:0?security=tls")).is_err());
    }

    #[test]
    fn parses_list_and_reports_skipped() {
        let text = format!(
            "{}\n# комментарий\nне ссылка\nvless://{UUID}@example.org:443?security=tls&type=ws#WS\n\n{}#Второй",
            link(""),
            link("")
        );
        let report = parse_links(&text);
        assert_eq!(report.profiles.len(), 2);
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(report.skipped[0].reason, "не vless-ссылка");
        assert_eq!(report.skipped[1].name, "WS");
        assert!(report.skipped[1].reason.contains("network=ws"));
        assert_eq!(report.profiles[1].name, "Второй");
    }

    #[test]
    fn detects_links_case_insensitively() {
        assert!(contains_links(&link("")));
        assert!(contains_links("VLESS://example"));
        assert!(!contains_links("https://example.com"));
        assert!(!contains_links(""));
    }

    #[test]
    fn percent_decoding_keeps_plus() {
        assert_eq!(percent_decode("a%20b%2Fc+d"), "a b/c+d");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
