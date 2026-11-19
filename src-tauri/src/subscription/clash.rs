//! Парсер подписки в формате Clash YAML: ключ `proxies` → `Vec<Profile>`.
//!
//! Узел Clash — плоское отображение, поэтому маппинг проще Happ-JSON.
//! Поддерживается тот же набор MVP — VLESS + Reality/TLS + TCP; узлы других
//! типов, транспортов и `tls: false` пропускаются с причиной, как и в
//! остальных парсерах (PRD §6.1). Неизвестные поля узла (udp, ws-opts и т.п.)
//! игнорируются.

use serde_yaml::Value;
use splitroute_protocol::{Profile, Protocol, TlsConfig, TlsKind, Transport};

use super::{is_uuid, profile_id, SkippedProfile, Subscription, SubscriptionError};

/// Пробует разобрать тело как Clash YAML. Ok(None) — тело не похоже на
/// Clash-конфиг (не YAML-отображение или нет ключа proxies): цепочка детекта
/// пробует следующий формат.
pub fn try_parse(raw: &str) -> Result<Option<Subscription>, SubscriptionError> {
    let root: Value = match serde_yaml::from_str(raw) {
        Ok(root) => root,
        // Совсем не YAML — пусть цепочка пробует следующий формат.
        Err(_) => return Ok(None),
    };
    let proxies = match root.get("proxies") {
        Some(proxies) => proxies,
        // YAML есть, но ключа proxies нет — не Clash-конфиг.
        None => return Ok(None),
    };
    Ok(Some(parse_proxies(proxies)?))
}

/// Разбирает значение ключа proxies. Пустое решение (все узлы пропущены) не
/// считается ошибкой здесь: финальную проверку делает вызывающий код (`finish`).
pub fn parse_proxies(proxies: &Value) -> Result<Subscription, SubscriptionError> {
    let items = proxies.as_sequence().ok_or_else(|| {
        SubscriptionError::InvalidFormat("ключ proxies не является списком".to_string())
    })?;
    if items.is_empty() {
        return Err(SubscriptionError::Empty);
    }

    let mut profiles = Vec::new();
    let mut skipped = Vec::new();
    for (offset, item) in items.iter().enumerate() {
        match convert_proxy(item) {
            Ok(profile) => profiles.push(profile),
            Err(reason) => skipped.push(SkippedProfile {
                index: offset + 1,
                name: item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                reason,
            }),
        }
    }

    Ok(Subscription { profiles, skipped })
}

/// Конвертирует один узел Clash в профиль; ошибка — причина пропуска узла.
fn convert_proxy(item: &Value) -> Result<Profile, String> {
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Профиль")
        .trim()
        .to_string();
    let label = if name.is_empty() {
        "профиль".to_string()
    } else {
        name.clone()
    };

    let proxy_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    if proxy_type.is_empty() {
        return Err(format!("{label}: не указан type"));
    }
    if proxy_type != "vless" {
        return Err(format!("{label}: type={proxy_type} не поддерживается"));
    }

    let server = item
        .get("server")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label}: нет server"))?
        .to_string();
    let port = item
        .get("port")
        .and_then(Value::as_u64)
        .filter(|port| (1..=65535).contains(port))
        .ok_or_else(|| format!("{label}: некорректный port"))? as u16;

    let uuid = item
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|value| is_uuid(value))
        .ok_or_else(|| format!("{label}: некорректный uuid"))?
        .to_string();
    let flow = item
        .get("flow")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let network = item.get("network").and_then(Value::as_str).unwrap_or("tcp");
    if network != "tcp" {
        return Err(format!("{label}: network={network} не поддерживается"));
    }

    if item.get("skip-cert-verify").and_then(Value::as_bool) == Some(true) {
        return Err(format!("{label}: skip-cert-verify=true не поддерживается"));
    }

    let tls = build_tls(item, &server).map_err(|reason| format!("{label}: {reason}"))?;

    let profile = Profile {
        id: profile_id(&name, &server, port, &uuid),
        name,
        protocol: Protocol::Vless,
        server,
        port,
        uuid,
        flow,
        tls,
        transport: Transport::Tcp,
        // Исходный узел — в кэш; при экзотических YAML-типах конвертация в
        // JSON может не удаться, тогда raw остаётся пустым.
        raw: serde_json::to_value(item).ok(),
    };

    Ok(profile)
}

/// TLS-параметры узла: `reality-opts.public-key` → Reality, `tls: true` → TLS,
/// остальное (включая `tls: false`) не поддерживается.
fn build_tls(item: &Value, server: &str) -> Result<TlsConfig, String> {
    if let Some(reality) = item.get("reality-opts") {
        let public_key = reality
            .get("public-key")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "в reality-opts нет public-key".to_string())?;
        return Ok(TlsConfig {
            kind: TlsKind::Reality,
            server_name: item
                .get("servername")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(server)
                .to_string(),
            public_key: Some(public_key.to_string()),
            short_id: reality
                .get("short-id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            fingerprint: item
                .get("client-fingerprint")
                .and_then(Value::as_str)
                .map(str::to_string),
            alpn: Vec::new(),
        });
    }
    if item.get("tls").and_then(Value::as_bool) == Some(true) {
        return Ok(TlsConfig {
            kind: TlsKind::Tls,
            server_name: item
                .get("servername")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(server)
                .to_string(),
            public_key: None,
            short_id: None,
            fingerprint: item
                .get("client-fingerprint")
                .and_then(Value::as_str)
                .map(str::to_string),
            alpn: item
                .get("alpn")
                .and_then(Value::as_sequence)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    Err("tls=false не поддерживается".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "d811a5ee-1d55-4dca-bd7f-f68b768a2241";

    /// Значение ключа `proxies` из YAML-документа — для parse_proxies.
    fn proxies_value(raw: &str) -> Value {
        serde_yaml::from_str::<Value>(raw)
            .expect("YAML с ключом proxies")
            .get("proxies")
            .cloned()
            .expect("ключ proxies есть")
    }

    #[test]
    fn parses_reality_node() {
        let raw = "\
port: 7890
proxies:
  - name: Тестовая локация A
    type: vless
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    flow: xtls-rprx-vision
    network: tcp
    tls: true
    servername: example.com
    client-fingerprint: chrome
    reality-opts:
      public-key: public-key
      short-id: 00e5e640a81a9ac1
    udp: true
    packet-encoding: xudp
";
        let subscription = try_parse(raw).unwrap().expect("Clash-конфиг");
        assert!(subscription.skipped.is_empty());
        let profile = &subscription.profiles[0];
        assert_eq!(profile.name, "Тестовая локация A");
        assert_eq!(profile.protocol, Protocol::Vless);
        assert_eq!(profile.server, "example.com");
        assert_eq!(profile.port, 443);
        assert_eq!(profile.uuid, UUID);
        assert_eq!(profile.flow.as_deref(), Some("xtls-rprx-vision"));
        assert_eq!(profile.tls.kind, TlsKind::Reality);
        assert_eq!(profile.tls.server_name, "example.com");
        assert_eq!(profile.tls.public_key.as_deref(), Some("public-key"));
        assert_eq!(profile.tls.short_id.as_deref(), Some("00e5e640a81a9ac1"));
        assert_eq!(profile.tls.fingerprint.as_deref(), Some("chrome"));
        assert!(profile.tls.alpn.is_empty());
        assert_eq!(profile.transport, Transport::Tcp);
        assert!(profile.id.starts_with("profile-"));
        assert!(profile.raw.is_some());
    }

    #[test]
    fn parses_tls_node_with_alpn() {
        let raw = "\
proxies:
  - name: TLS
    type: vless
    server: 2001:db8::1
    port: 8443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    tls: true
    servername: cdn.example
    client-fingerprint: chrome
    alpn: [h2, http/1.1]
";
        let value = proxies_value(raw);
        let subscription = parse_proxies(&value).unwrap();
        let profile = &subscription.profiles[0];
        assert_eq!(profile.server, "2001:db8::1");
        assert_eq!(profile.port, 8443);
        assert_eq!(profile.tls.kind, TlsKind::Tls);
        assert_eq!(profile.tls.server_name, "cdn.example");
        assert_eq!(profile.tls.alpn, vec!["h2", "http/1.1"]);
        assert_eq!(profile.tls.fingerprint.as_deref(), Some("chrome"));
        assert!(profile.tls.public_key.is_none());
        assert!(profile.flow.is_none());
    }

    #[test]
    fn skips_unsupported_types_with_reason() {
        let value = proxies_value(
            "\
proxies:
  - name: VMess
    type: vmess
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
  - name: Trojan
    type: trojan
    server: example.org
    port: 443
    password: secret
",
        );
        let subscription = parse_proxies(&value).unwrap();
        assert!(subscription.profiles.is_empty());
        assert_eq!(subscription.skipped.len(), 2);
        assert_eq!(subscription.skipped[0].index, 1);
        assert_eq!(subscription.skipped[0].name, "VMess");
        assert_eq!(
            subscription.skipped[0].reason,
            "VMess: type=vmess не поддерживается"
        );
        assert_eq!(
            subscription.skipped[1].reason,
            "Trojan: type=trojan не поддерживается"
        );
    }

    #[test]
    fn skips_ws_transport() {
        let value = proxies_value(
            "\
proxies:
  - name: WS
    type: vless
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    tls: true
    network: ws
    ws-opts:
      path: /path
",
        );
        let subscription = parse_proxies(&value).unwrap();
        assert_eq!(
            subscription.skipped[0].reason,
            "WS: network=ws не поддерживается"
        );
    }

    #[test]
    fn skips_plaintext_node() {
        let value = proxies_value(
            "\
proxies:
  - name: Plain
    type: vless
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    tls: false
",
        );
        let subscription = parse_proxies(&value).unwrap();
        assert_eq!(
            subscription.skipped[0].reason,
            "Plain: tls=false не поддерживается"
        );
    }

    #[test]
    fn skips_insecure_tls() {
        let value = proxies_value(
            "\
proxies:
  - name: Insecure
    type: vless
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    tls: true
    skip-cert-verify: true
",
        );
        let subscription = parse_proxies(&value).unwrap();
        assert_eq!(
            subscription.skipped[0].reason,
            "Insecure: skip-cert-verify=true не поддерживается"
        );
    }

    #[test]
    fn skips_invalid_uuid() {
        let value = proxies_value(
            "\
proxies:
  - name: Плохой uuid
    type: vless
    server: example.com
    port: 443
    uuid: not-a-uuid
",
        );
        let subscription = parse_proxies(&value).unwrap();
        assert_eq!(subscription.skipped[0].name, "Плохой uuid");
        assert_eq!(
            subscription.skipped[0].reason,
            "Плохой uuid: некорректный uuid"
        );
    }

    #[test]
    fn reality_requires_public_key() {
        let value = proxies_value(
            "\
proxies:
  - name: Broken
    type: vless
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    tls: true
    reality-opts:
      short-id: 00e5e640a81a9ac1
",
        );
        let subscription = parse_proxies(&value).unwrap();
        assert_eq!(
            subscription.skipped[0].reason,
            "Broken: в reality-opts нет public-key"
        );
    }

    #[test]
    fn try_parse_rejects_non_clash_bodies() {
        // Не YAML-отображение.
        assert!(try_parse("hello world").unwrap().is_none());
        // YAML, но без ключа proxies.
        assert!(try_parse("port: 7890").unwrap().is_none());
    }

    #[test]
    fn empty_proxies_list_is_empty_error() {
        let value = proxies_value("proxies: []");
        assert_eq!(parse_proxies(&value).unwrap_err(), SubscriptionError::Empty);
    }

    #[test]
    fn scalar_proxies_is_invalid_format() {
        let value = proxies_value("proxies: 5");
        assert_eq!(
            parse_proxies(&value).unwrap_err(),
            SubscriptionError::InvalidFormat("ключ proxies не является списком".to_string())
        );
    }
}
