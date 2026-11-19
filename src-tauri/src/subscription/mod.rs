//! Парсер подписки: Happ-JSON, Clash YAML или список `vless://`-ссылок →
//! `Vec<Profile>`.
//!
//! Маппинг повторяет проверенный спайком `spike/convert.py` (VLESS + Reality/TLS +
//! TCP). Профили без outbound `proxy`, с другим протоколом или транспортом
//! пропускаются с причиной, а не роняют обновление (PRD §6.1).

use serde::Serialize;
use serde_json::Value;
use splitroute_protocol::{Profile, Protocol, TlsConfig, TlsKind, Transport};

pub mod base64;
pub mod clash;
pub mod fetch;
pub mod vless;

pub use fetch::{fetch_and_parse, FetchError, FetchRequest, ReqwestFetcher, SubscriptionFetcher};

/// Ошибка разбора ответа подписки целиком.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SubscriptionError {
    #[error("невалидный JSON: {0}")]
    InvalidJson(String),
    #[error("неизвестный формат ответа: {0}")]
    InvalidFormat(String),
    #[error("ожидался JSON-массив профилей")]
    NotAnArray,
    #[error("массив профилей пуст")]
    Empty,
    #[error("ни один профиль не сконвертирован (пропущено {skipped} из {total})")]
    NoProfiles { total: usize, skipped: usize },
}

/// Профиль, пропущенный при разборе, с причиной.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedProfile {
    pub index: usize,
    pub name: String,
    pub reason: String,
}

/// Результат разбора: профили и список пропусков.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub profiles: Vec<Profile>,
    pub skipped: Vec<SkippedProfile>,
}

/// Разбирает подписку и возвращает только профили.
pub fn parse_subscription(raw: &str) -> Result<Vec<Profile>, SubscriptionError> {
    Ok(parse_subscription_report(raw)?.profiles)
}

/// Разбирает подписку и возвращает профили вместе с пропусками.
///
/// Формат определяется по телу: Happ-JSON, Clash YAML, base64-список
/// `vless://` или тот же список открытым текстом. Так клиент работает и с
/// Happ User-Agent (JSON), и с собственным `SplitRoute/*` (base64), и с
/// подписками в формате Clash.
pub fn parse_subscription_report(raw: &str) -> Result<Subscription, SubscriptionError> {
    let trimmed = raw.trim().trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(SubscriptionError::Empty);
    }
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        return parse_json_report(trimmed);
    }
    if vless::contains_links(trimmed) {
        return finish(vless::parse_links(trimmed));
    }
    if let Some(subscription) = clash::try_parse(trimmed)? {
        return finish(subscription);
    }
    if let Some(decoded) = base64::decode(trimmed) {
        let text = String::from_utf8_lossy(&decoded);
        if vless::contains_links(&text) {
            return finish(vless::parse_links(&text));
        }
    }
    Err(SubscriptionError::InvalidFormat(
        "ожидался Happ-JSON, Clash YAML или список vless-ссылок (base64 или открытым текстом)"
            .to_string(),
    ))
}

/// Единый финал для vless-списков и Clash: если ни один узел не
/// сконвертирован, это ошибка `NoProfiles`, а не пустой успех.
fn finish(subscription: Subscription) -> Result<Subscription, SubscriptionError> {
    if subscription.profiles.is_empty() {
        return Err(SubscriptionError::NoProfiles {
            total: subscription.profiles.len() + subscription.skipped.len(),
            skipped: subscription.skipped.len(),
        });
    }
    Ok(subscription)
}

fn parse_json_report(raw: &str) -> Result<Subscription, SubscriptionError> {
    let payload: Value = match serde_json::from_str(raw) {
        Ok(payload) => payload,
        Err(error) => {
            // YAML — надмножество JSON: тело в фигурных скобках без кавычек
            // ключей может оказаться Clash-конфигом в flow-стиле.
            return match clash::try_parse(raw)? {
                Some(subscription) => Ok(subscription),
                None => Err(SubscriptionError::InvalidJson(error.to_string())),
            };
        }
    };
    // JSON-объект с ключом proxies — Clash в JSON-нотации, а не Happ-профиль.
    if payload
        .as_object()
        .is_some_and(|object| object.contains_key("proxies"))
    {
        let root: serde_yaml::Value = serde_yaml::from_str(raw)
            .map_err(|error| SubscriptionError::InvalidJson(error.to_string()))?;
        let proxies = root
            .get("proxies")
            .ok_or_else(|| SubscriptionError::InvalidFormat("нет ключа proxies".to_string()))?;
        return finish(clash::parse_proxies(proxies)?);
    }
    let items = payload.as_array().ok_or(SubscriptionError::NotAnArray)?;
    if items.is_empty() {
        return Err(SubscriptionError::Empty);
    }

    let mut profiles = Vec::new();
    let mut skipped = Vec::new();
    for (offset, item) in items.iter().enumerate() {
        match convert_profile(item) {
            Ok(profile) => profiles.push(profile),
            Err(reason) => skipped.push(SkippedProfile {
                index: offset + 1,
                name: item
                    .get("remarks")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                reason,
            }),
        }
    }

    if profiles.is_empty() {
        return Err(SubscriptionError::NoProfiles {
            total: items.len(),
            skipped: skipped.len(),
        });
    }

    // Индекс профиля в подписке не входит в модель, но нужен в отчёте о пропусках.
    skipped.sort_by_key(|entry| entry.index);
    Ok(Subscription { profiles, skipped })
}

fn convert_profile(item: &Value) -> Result<Profile, String> {
    let remarks = item
        .get("remarks")
        .and_then(Value::as_str)
        .unwrap_or("Профиль")
        .trim()
        .to_string();
    let label = if remarks.is_empty() {
        "профиль".to_string()
    } else {
        remarks.clone()
    };

    let proxy =
        find_proxy_outbound(item).ok_or_else(|| format!("{label}: нет outbound с тегом proxy"))?;

    let protocol = proxy.get("protocol").and_then(Value::as_str).unwrap_or("");
    if protocol != "vless" {
        return Err(format!("{label}: protocol={protocol} не поддерживается"));
    }

    let vnext = proxy
        .pointer("/settings/vnext")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| format!("{label}: пустой settings.vnext"))?;
    let endpoint = vnext[0]
        .as_object()
        .ok_or_else(|| format!("{label}: некорректный элемент vnext"))?;

    let server = endpoint
        .get("address")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label}: нет address"))?
        .to_string();
    let port = endpoint
        .get("port")
        .and_then(Value::as_u64)
        .filter(|port| (1..=65535).contains(port))
        .ok_or_else(|| format!("{label}: некорректный port"))? as u16;

    let user = endpoint
        .get("users")
        .and_then(Value::as_array)
        .and_then(|users| users.first())
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{label}: нет users"))?;

    let uuid = user
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| is_uuid(value))
        .ok_or_else(|| format!("{label}: некорректный uuid"))?
        .to_string();
    let flow = user
        .get("flow")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let stream = proxy.get("streamSettings").unwrap_or(&Value::Null);
    let network = stream
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("tcp");
    if network != "tcp" {
        return Err(format!("{label}: network={network} не поддерживается"));
    }

    let security = stream
        .get("security")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let tls =
        build_tls(stream, security, &server).map_err(|reason| format!("{label}: {reason}"))?;

    let profile = Profile {
        id: profile_id(&remarks, &server, port, &uuid),
        name: remarks,
        protocol: Protocol::Vless,
        server,
        port,
        uuid,
        flow,
        tls,
        transport: Transport::Tcp,
        raw: Some(item.clone()),
    };

    Ok(profile)
}

fn find_proxy_outbound(item: &Value) -> Option<&Value> {
    item.get("outbounds")?
        .as_array()?
        .iter()
        .find(|outbound| outbound.get("tag").and_then(Value::as_str) == Some("proxy"))
}

fn build_tls(stream: &Value, security: &str, address: &str) -> Result<TlsConfig, String> {
    match security {
        "reality" => {
            let reality = stream.get("realitySettings").unwrap_or(&Value::Null);
            let public_key = reality
                .get("publicKey")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "в realitySettings нет publicKey".to_string())?;
            Ok(TlsConfig {
                kind: TlsKind::Reality,
                server_name: reality
                    .get("serverName")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address)
                    .to_string(),
                public_key: Some(public_key.to_string()),
                short_id: reality
                    .get("shortId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                fingerprint: reality
                    .get("fingerprint")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                alpn: Vec::new(),
            })
        }
        "tls" => {
            let settings = stream.get("tlsSettings").unwrap_or(&Value::Null);
            Ok(TlsConfig {
                kind: TlsKind::Tls,
                server_name: settings
                    .get("serverName")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address)
                    .to_string(),
                public_key: None,
                short_id: None,
                fingerprint: settings
                    .get("fingerprint")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                alpn: settings
                    .get("alpn")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        }
        other => Err(format!("неподдерживаемая security: {other}")),
    }
}

/// UUID в каноническом виде 8-4-4-4-12.
pub(crate) fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Стабильный идентификатор профиля: FNV-1a по имени и параметрам подключения.
pub(crate) fn profile_id(name: &str, server: &str, port: u16, uuid: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in [name, server, &port.to_string(), uuid] {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("profile-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_profile(remarks: &str) -> Value {
        json!({
            "remarks": remarks,
            "outbounds": [
                {
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": "example.com",
                            "port": 443,
                            "users": [{
                                "id": "d811a5ee-1d55-4dca-bd7f-f68b768a2241",
                                "flow": "xtls-rprx-vision"
                            }]
                        }]
                    },
                    "streamSettings": {
                        "network": "tcp",
                        "security": "reality",
                        "realitySettings": {
                            "serverName": "example.com",
                            "publicKey": "public-key",
                            "shortId": "00e5e640a81a9ac1",
                            "fingerprint": "chrome"
                        }
                    }
                },
                { "tag": "direct", "protocol": "freedom" }
            ]
        })
    }

    #[test]
    fn parses_valid_profile() {
        let raw = json!([valid_profile("Тестовая локация A")]).to_string();
        let profiles = parse_subscription(&raw).unwrap();
        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];
        assert_eq!(profile.name, "Тестовая локация A");
        assert_eq!(profile.protocol, Protocol::Vless);
        assert_eq!(profile.server, "example.com");
        assert_eq!(profile.port, 443);
        assert_eq!(profile.flow.as_deref(), Some("xtls-rprx-vision"));
        assert_eq!(profile.tls.kind, TlsKind::Reality);
        assert_eq!(profile.tls.public_key.as_deref(), Some("public-key"));
        assert_eq!(profile.tls.short_id.as_deref(), Some("00e5e640a81a9ac1"));
        assert_eq!(profile.tls.server_name, "example.com");
        assert_eq!(profile.transport, Transport::Tcp);
        assert!(profile.raw.is_some());
    }

    #[test]
    fn empty_array_is_error() {
        assert_eq!(
            parse_subscription("[]").unwrap_err(),
            SubscriptionError::Empty
        );
    }

    #[test]
    fn not_an_array_is_error() {
        let error = parse_subscription(r#"{"remarks":"x"}"#).unwrap_err();
        assert_eq!(error, SubscriptionError::NotAnArray);
    }

    #[test]
    fn broken_json_is_error() {
        assert!(matches!(
            parse_subscription("{oops"),
            Err(SubscriptionError::InvalidJson(_))
        ));
    }

    #[test]
    fn profile_without_proxy_is_skipped_with_reason() {
        let raw = json!([
            { "remarks": "без proxy", "outbounds": [{ "tag": "direct", "protocol": "freedom" }] },
            valid_profile("рабочий")
        ])
        .to_string();
        let report = parse_subscription_report(&raw).unwrap();
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].name, "без proxy");
        assert!(report.skipped[0].reason.contains("нет outbound"));
    }

    #[test]
    fn unsupported_network_is_skipped() {
        let mut profile = valid_profile("ws");
        profile["outbounds"][0]["streamSettings"]["network"] = json!("ws");
        let raw = json!([profile, valid_profile("рабочий")]).to_string();
        let report = parse_subscription_report(&raw).unwrap();
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].reason.contains("network=ws"));
    }

    #[test]
    fn all_profiles_skipped_is_error() {
        let raw = json!([{ "remarks": "пусто", "outbounds": [] }]).to_string();
        assert_eq!(
            parse_subscription(&raw).unwrap_err(),
            SubscriptionError::NoProfiles {
                total: 1,
                skipped: 1
            }
        );
    }

    #[test]
    fn profile_id_is_stable() {
        let raw = json!([valid_profile("A")]).to_string();
        let first = parse_subscription(&raw).unwrap()[0].id.clone();
        let second = parse_subscription(&raw).unwrap()[0].id.clone();
        assert_eq!(first, second);
        assert!(first.starts_with("profile-"));
    }

    #[test]
    fn uuid_validator() {
        assert!(is_uuid("d811a5ee-1d55-4dca-bd7f-f68b768a2241"));
        assert!(!is_uuid("d811a5ee1d554dcabd7ff68b768a2241"));
        assert!(!is_uuid("zzzzzzzz-1d55-4dca-bd7f-f68b768a2241"));
    }

    const BASE64_LINK: &str = "dmxlc3M6Ly9kODExYTVlZS0xZDU1LTRkY2EtYmQ3Zi1mNjhiNzY4YTIyNDFAZXhhbXBsZS5jb206NDQzP2VuY3J5cHRpb249bm9uZSZzZWN1cml0eT1yZWFsaXR5JnNuaT1leGFtcGxlLmNvbSZmcD1jaHJvbWUmcGJrPXB1YmxpYy1rZXkmc2lkPTAwZTVlNjQwYTgxYTlhYzEmdHlwZT10Y3AjVGVzdCUyMExvY2F0aW9u";
    const BASE64_WS: &str = "dmxlc3M6Ly9kODExYTVlZS0xZDU1LTRkY2EtYmQ3Zi1mNjhiNzY4YTIyNDFAZXhhbXBsZS5vcmc6NDQzP3NlY3VyaXR5PXRscyZ0eXBlPXdzI1dT";

    #[test]
    fn base64_vless_subscription_is_parsed() {
        let report = parse_subscription_report(BASE64_LINK).unwrap();
        assert_eq!(report.profiles.len(), 1);
        let profile = &report.profiles[0];
        assert_eq!(profile.name, "Test Location");
        assert_eq!(profile.server, "example.com");
        assert_eq!(profile.port, 443);
        assert_eq!(profile.tls.kind, TlsKind::Reality);
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn plain_text_vless_subscription_is_parsed() {
        let link = "vless://d811a5ee-1d55-4dca-bd7f-f68b768a2241@example.com:443?encryption=none&security=reality&sni=example.com&fp=chrome&pbk=public-key&sid=00e5e640a81a9ac1&type=tcp#Test%20Location";
        let report = parse_subscription_report(link).unwrap();
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.profiles[0].name, "Test Location");
    }

    #[test]
    fn base64_with_only_unsupported_links_is_no_profiles() {
        assert_eq!(
            parse_subscription_report(BASE64_WS).unwrap_err(),
            SubscriptionError::NoProfiles {
                total: 1,
                skipped: 1
            }
        );
    }

    #[test]
    fn unknown_body_is_invalid_format() {
        assert!(matches!(
            parse_subscription_report("hello world"),
            Err(SubscriptionError::InvalidFormat(_))
        ));
        // base64 от "hello world" — валидный base64, но не ссылки.
        assert!(matches!(
            parse_subscription_report("aGVsbG8gd29ybGQ="),
            Err(SubscriptionError::InvalidFormat(_))
        ));
    }

    const CLASH_REALITY_YAML: &str = "\
port: 7890
proxies:
  - name: Тестовая локация A
    type: vless
    server: example.com
    port: 443
    uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241
    flow: xtls-rprx-vision
    tls: true
    servername: example.com
    client-fingerprint: chrome
    reality-opts:
      public-key: public-key
      short-id: 00e5e640a81a9ac1
";

    #[test]
    fn clash_yaml_subscription_is_parsed() {
        let report = parse_subscription_report(CLASH_REALITY_YAML).unwrap();
        assert_eq!(report.profiles.len(), 1);
        assert!(report.skipped.is_empty());
        let profile = &report.profiles[0];
        assert_eq!(profile.name, "Тестовая локация A");
        assert_eq!(profile.server, "example.com");
        assert_eq!(profile.port, 443);
        assert_eq!(profile.tls.kind, TlsKind::Reality);
        assert_eq!(profile.tls.public_key.as_deref(), Some("public-key"));
        assert_eq!(profile.tls.short_id.as_deref(), Some("00e5e640a81a9ac1"));
    }

    #[test]
    fn clash_json_notation_is_parsed_as_clash() {
        let raw = r#"{"proxies":[{"name":"TLS","type":"vless","server":"example.com","port":443,"uuid":"d811a5ee-1d55-4dca-bd7f-f68b768a2241","tls":true}]}"#;
        let report = parse_subscription_report(raw).unwrap();
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.profiles[0].name, "TLS");
        assert_eq!(report.profiles[0].tls.kind, TlsKind::Tls);
        assert_eq!(report.profiles[0].tls.server_name, "example.com");
    }

    #[test]
    fn clash_flow_style_is_parsed_as_clash() {
        // Не JSON: ключи без кавычек — YAML flow-стиль.
        let raw = "{port: 7890, proxies: [{name: TLS, type: vless, server: example.com, port: 443, uuid: d811a5ee-1d55-4dca-bd7f-f68b768a2241, tls: true}]}";
        let report = parse_subscription_report(raw).unwrap();
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.profiles[0].name, "TLS");
        assert_eq!(report.profiles[0].tls.kind, TlsKind::Tls);
    }

    #[test]
    fn clash_with_only_unsupported_nodes_is_no_profiles() {
        let raw = "\
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
";
        assert_eq!(
            parse_subscription_report(raw).unwrap_err(),
            SubscriptionError::NoProfiles {
                total: 2,
                skipped: 2
            }
        );
    }
}
