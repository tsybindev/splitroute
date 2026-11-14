//! Генератор конфига sing-box: чистая функция `Settings + Profile + AppRule[] → JSON`.
//!
//! Схема повторяет проверенный спайком `spike/convert.py` (sing-box 1.14.4,
//! `sing-box check`). Все внешние данные приходят типами, строки не исполняются.

use serde_json::{json, Map, Value};
use splitroute_protocol::{AppRule, DnsMode, Profile, Protocol, Settings, TlsKind, Transport};

/// DoT-сервер для выбранных процессов (идёт через proxy).
pub const REMOTE_DNS: &str = "1.1.1.1";
/// Адрес TUN для IPv4.
pub const TUN_ADDRESS_V4: &str = "172.19.0.1/30";
/// Адрес TUN для IPv6 (только если IPv6 включён).
pub const TUN_ADDRESS_V6: &str = "fdfe:dcba:9876::1/126";
/// Локальный адрес Clash API: только loopback, наружу не слушает.
pub const CLASH_API_HOST: &str = "127.0.0.1";

/// Параметры локального Clash API ядра для статистики трафика (PRD §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashApi {
    pub port: u16,
    pub secret: String,
}

/// Ошибка генерации конфига.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("неподдерживаемый протокол профиля")]
    UnsupportedProtocol,
    #[error("неподдерживаемый транспорт профиля")]
    UnsupportedTransport,
    #[error("для Reality нужен public_key")]
    MissingRealityKey,
    #[error("некорректные настройки: {0}")]
    InvalidSettings(String),
}

/// Собирает конфиг sing-box. `rules` — полный список правил allowlist; выключенные
/// игнорируются. `clash_api` включает локальный API статистики, если задан.
pub fn build_config(
    settings: &Settings,
    profile: &Profile,
    rules: &[AppRule],
    clash_api: Option<&ClashApi>,
) -> Result<Value, ConfigError> {
    validate(settings, profile)?;

    let active: Vec<&AppRule> = rules.iter().filter(|rule| rule.enabled).collect();

    let mut config = json!({
        "log": { "level": json!(settings.log_level), "timestamp": true },
        "dns": build_dns(settings),
        "inbounds": build_inbounds(settings),
        "outbounds": build_outbounds(settings, profile)?,
        "route": build_route(settings, &active),
    });
    if let Some(clash_api) = clash_api {
        config["experimental"] = json!({
            "clash_api": {
                "external_controller": format!("{CLASH_API_HOST}:{}", clash_api.port),
                "secret": clash_api.secret,
            }
        });
    }
    Ok(config)
}

fn validate(settings: &Settings, profile: &Profile) -> Result<(), ConfigError> {
    if profile.protocol != Protocol::Vless {
        return Err(ConfigError::UnsupportedProtocol);
    }
    if profile.transport != Transport::Tcp {
        return Err(ConfigError::UnsupportedTransport);
    }
    if profile.tls.kind == TlsKind::Reality
        && profile.tls.public_key.as_deref().is_none_or(str::is_empty)
    {
        return Err(ConfigError::MissingRealityKey);
    }
    if !(576..=9000).contains(&settings.mtu) {
        return Err(ConfigError::InvalidSettings(format!(
            "MTU вне диапазона 576..9000: {}",
            settings.mtu
        )));
    }
    if settings.mixed_inbound_enabled && settings.mixed_port == 0 {
        return Err(ConfigError::InvalidSettings(
            "порт mixed-inbound не может быть 0".to_string(),
        ));
    }
    Ok(())
}

/// DNS-политика (PRD §6.4). Per-process DNS не поддерживается, поэтому правил
/// по процессам здесь нет: либо весь DNS через прокси, либо системный DNS.
fn build_dns(settings: &Settings) -> Value {
    match settings.dns_mode {
        // Весь DNS через прокси; локальные имена резолвим системно.
        DnsMode::Proxy => json!({
            "servers": [
                {
                    "tag": "remote",
                    "type": "tls",
                    "server": REMOTE_DNS,
                    "detour": "proxy",
                },
                { "tag": "local", "type": "local" },
            ],
            "rules": [
                { "domain": ["localhost"], "server": "local" },
                { "domain_suffix": [".local", "home.arpa"], "server": "local" },
            ],
            "final": "remote",
        }),
        // Только системный DNS, без remote и правил.
        DnsMode::System => json!({
            "servers": [{ "tag": "local", "type": "local" }],
            "rules": [],
            "final": "local",
        }),
    }
}

fn build_inbounds(settings: &Settings) -> Value {
    let mut address = vec![json!(TUN_ADDRESS_V4)];
    if settings.ipv6_enabled {
        address.push(json!(TUN_ADDRESS_V6));
    }

    let mut inbounds = vec![json!({
        "type": "tun",
        "tag": "tun-in",
        "address": address,
        "mtu": settings.mtu,
        "auto_route": true,
        "strict_route": true,
        "stack": "mixed",
    })];

    if settings.mixed_inbound_enabled {
        inbounds.push(json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": settings.mixed_port,
        }));
    }

    Value::Array(inbounds)
}

fn build_outbounds(settings: &Settings, profile: &Profile) -> Result<Value, ConfigError> {
    let mut proxy = json!({
        "type": "vless",
        "tag": "proxy",
        "server": profile.server,
        "server_port": profile.port,
        "uuid": profile.uuid,
    });
    if let Some(flow) = profile.flow.as_deref().filter(|flow| !flow.is_empty()) {
        proxy["flow"] = json!(flow);
    }
    proxy["tls"] = build_tls(profile)?;

    let mut outbounds = vec![proxy, json!({ "type": "direct", "tag": "direct" })];

    // Без IPv6 TUN выбранные приложения не должны утекать по IPv6 — блокируем.
    if !settings.ipv6_enabled {
        outbounds.push(json!({ "type": "block", "tag": "block" }));
    }

    Ok(Value::Array(outbounds))
}

fn build_tls(profile: &Profile) -> Result<Value, ConfigError> {
    match profile.tls.kind {
        TlsKind::Reality => {
            let public_key = profile
                .tls
                .public_key
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or(ConfigError::MissingRealityKey)?;
            Ok(json!({
                "enabled": true,
                "server_name": profile.tls.server_name,
                "utls": {
                    "enabled": true,
                    "fingerprint": profile.tls.fingerprint.as_deref().unwrap_or("chrome"),
                },
                "reality": {
                    "enabled": true,
                    "public_key": public_key,
                    "short_id": profile.tls.short_id.as_deref().unwrap_or(""),
                },
            }))
        }
        TlsKind::Tls => {
            let mut tls = json!({
                "enabled": true,
                "server_name": profile.tls.server_name,
            });
            if !profile.tls.alpn.is_empty() {
                tls["alpn"] = json!(profile.tls.alpn);
            }
            if let Some(fingerprint) = profile.tls.fingerprint.as_deref() {
                tls["utls"] = json!({ "enabled": true, "fingerprint": fingerprint });
            }
            Ok(tls)
        }
    }
}

fn build_route(settings: &Settings, rules: &[&AppRule]) -> Value {
    let mut route_rules = Vec::new();

    // В режиме proxy перехватываем DNS из TUN и отдаём его в dns.final=remote.
    // В режиме system DNS не перехватываем: приложения используют DNS ОС.
    if settings.dns_mode == DnsMode::Proxy {
        route_rules.push(json!({ "port": [53], "action": "hijack-dns" }));
    }

    // Локальный трафик и приватные сети — всегда напрямую и раньше
    // process-правил, чтобы не загнать петлю mixed-inbound в прокси.
    route_rules.push(json!({ "domain": ["localhost"], "outbound": "direct" }));
    route_rules.push(json!({ "ip_is_private": true, "outbound": "direct" }));

    if !settings.ipv6_enabled {
        for rule in rules {
            if let Some(mut selectors) = selectors(rule) {
                selectors.insert("ip_cidr".to_string(), json!(["::/0"]));
                selectors.insert("outbound".to_string(), json!("block"));
                route_rules.push(Value::Object(selectors));
            }
        }
    }

    for rule in rules {
        if let Some(mut selectors) = selectors(rule) {
            selectors.insert("outbound".to_string(), json!("proxy"));
            route_rules.push(Value::Object(selectors));
        }
    }

    json!({
        "rules": route_rules,
        "final": "direct",
        "auto_detect_interface": true,
        "default_domain_resolver": { "server": "local" },
    })
}

/// Селекторы правила в терминах sing-box. `None`, если правило пустое.
fn selectors(rule: &AppRule) -> Option<Map<String, Value>> {
    let mut map = Map::new();
    if !rule.process_name.is_empty() {
        map.insert("process_name".to_string(), json!(rule.process_name));
    }
    if !rule.process_path.is_empty() {
        map.insert("process_path".to_string(), json!(rule.process_path));
    }
    if !rule.path_regex.is_empty() {
        map.insert("process_path_regex".to_string(), json!(rule.path_regex));
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splitroute_protocol::{Platform, TlsConfig};

    fn profile() -> Profile {
        Profile {
            id: "profile-1".to_string(),
            name: "Тест".to_string(),
            protocol: Protocol::Vless,
            server: "203.0.113.10".to_string(),
            port: 443,
            uuid: "d811a5ee-1d55-4dca-bd7f-f68b768a2241".to_string(),
            flow: Some("xtls-rprx-vision".to_string()),
            tls: TlsConfig {
                kind: TlsKind::Reality,
                server_name: "example.com".to_string(),
                public_key: Some("public-key".to_string()),
                short_id: Some("00e5e640a81a9ac1".to_string()),
                fingerprint: Some("chrome".to_string()),
                alpn: Vec::new(),
            },
            transport: Transport::Tcp,
            raw: None,
        }
    }

    fn rule() -> AppRule {
        AppRule {
            id: "rule-chrome".to_string(),
            name: "Google Chrome".to_string(),
            platform: Platform::Macos,
            process_name: vec!["Google Chrome".to_string()],
            process_path: Vec::new(),
            path_regex: vec![r"^/Applications/Google Chrome\.app/".to_string()],
            enabled: true,
        }
    }

    #[test]
    fn default_config_has_tun_and_process_rule() {
        let config = build_config(&Settings::default(), &profile(), &[rule()], None).unwrap();
        assert_eq!(config["inbounds"][0]["type"], "tun");
        assert_eq!(config["inbounds"][0]["mtu"], 1500);
        assert_eq!(config["outbounds"][0]["type"], "vless");
        assert_eq!(config["route"]["final"], "direct");
        let rules = config["route"]["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|item| item["process_name"][0] == "Google Chrome"));
        assert!(rules.iter().any(|item| {
            item["process_path_regex"]
                .as_array()
                .is_some_and(|patterns| {
                    patterns.iter().any(|pattern| {
                        pattern
                            .as_str()
                            .is_some_and(|p| p.contains("Google Chrome"))
                    })
                })
        }));
    }

    #[test]
    fn disabled_rules_are_ignored() {
        let mut disabled = rule();
        disabled.enabled = false;
        let config = build_config(&Settings::default(), &profile(), &[disabled], None).unwrap();
        let rules = config["route"]["rules"].as_array().unwrap();
        assert_eq!(
            rules.len(),
            3,
            "hijack-dns, localhost и ip_is_private; правил по процессам нет"
        );
    }

    #[test]
    fn proxy_dns_goes_through_proxy_without_process_rules() {
        let config = build_config(&Settings::default(), &profile(), &[rule()], None).unwrap();
        let dns = &config["dns"];
        assert_eq!(dns["final"], "remote");
        let servers = dns["servers"].as_array().unwrap();
        assert!(servers.iter().any(|item| item["tag"] == "remote"
            && item["type"] == "tls"
            && item["detour"] == "proxy"));
        assert!(servers.iter().any(|item| item["tag"] == "local"));

        let dns_rules = dns["rules"].as_array().unwrap();
        assert!(dns_rules
            .iter()
            .all(|item| item.get("process_name").is_none()
                && item.get("process_path").is_none()
                && item.get("process_path_regex").is_none()));
        assert!(dns_rules.iter().any(|item| item["domain_suffix"]
            .as_array()
            .is_some_and(|suffixes| suffixes.iter().any(|s| s == ".local"))));

        let route_rules = config["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules[0]["action"], "hijack-dns");
        assert_eq!(route_rules[0]["port"][0], 53);
    }

    #[test]
    fn ipv6_disabled_blocks_selected_process() {
        let config = build_config(&Settings::default(), &profile(), &[rule()], None).unwrap();
        let rules = config["route"]["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|item| item["ip_cidr"][0] == "::/0" && item["outbound"] == "block"));
        assert!(config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "block"));
    }

    #[test]
    fn ipv6_enabled_adds_address_and_no_block() {
        let settings = Settings {
            ipv6_enabled: true,
            ..Settings::default()
        };
        let config = build_config(&settings, &profile(), &[rule()], None).unwrap();
        let address = config["inbounds"][0]["address"].as_array().unwrap();
        assert_eq!(address.len(), 2);
        assert!(config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["type"] != "block"));
    }

    #[test]
    fn system_dns_has_no_remote_server() {
        let settings = Settings {
            dns_mode: DnsMode::System,
            ..Settings::default()
        };
        let config = build_config(&settings, &profile(), &[rule()], None).unwrap();
        let servers = config["dns"]["servers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["tag"], "local");
        assert_eq!(config["dns"]["final"], "local");
        assert!(config["dns"]["rules"].as_array().unwrap().is_empty());
        assert!(
            config["route"]["rules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item.get("action").is_none() || item["action"] != "hijack-dns"),
            "в режиме system DNS не перехватывается"
        );
    }

    #[test]
    fn missing_reality_key_is_error() {
        let mut broken = profile();
        broken.tls.public_key = None;
        assert_eq!(
            build_config(&Settings::default(), &broken, &[], None),
            Err(ConfigError::MissingRealityKey)
        );
    }

    #[test]
    fn invalid_mtu_is_error() {
        let settings = Settings {
            mtu: 100,
            ..Settings::default()
        };
        assert!(matches!(
            build_config(&settings, &profile(), &[], None),
            Err(ConfigError::InvalidSettings(_))
        ));
    }

    #[test]
    fn mixed_inbound_can_be_disabled() {
        let settings = Settings {
            mixed_inbound_enabled: false,
            ..Settings::default()
        };
        let config = build_config(&settings, &profile(), &[], None).unwrap();
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn clash_api_listens_only_on_loopback_with_secret() {
        let api = ClashApi {
            port: 19090,
            secret: "0123456789abcdef0123456789abcdef".to_string(),
        };
        let config = build_config(&Settings::default(), &profile(), &[], Some(&api)).unwrap();
        assert_eq!(
            config["experimental"]["clash_api"]["external_controller"],
            "127.0.0.1:19090"
        );
        assert_eq!(
            config["experimental"]["clash_api"]["secret"],
            "0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn no_clash_api_block_without_endpoint() {
        let config = build_config(&Settings::default(), &profile(), &[], None).unwrap();
        assert!(config.get("experimental").is_none());
    }
}
