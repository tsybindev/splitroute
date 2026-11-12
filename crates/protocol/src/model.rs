//! Доменные модели: профиль подписки, правило приложения, настройки.
//!
//! Модели сериализуются в кэш приложения. Секреты (URL подписки, HWID) сюда не
//! входят: они хранятся отдельно и не попадают в диагностику (PRD §6.1).

use serde::{Deserialize, Serialize};

/// Протокол подключения. MVP обязан поддерживать VLESS + Reality + TCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Vless,
}

/// Транспорт профиля.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Tcp,
    Ws,
    Grpc,
    Http,
}

/// Тип TLS у профиля.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsKind {
    Tls,
    Reality,
}

/// Параметры TLS/Reality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsConfig {
    pub kind: TlsKind,
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub short_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alpn: Vec<String>,
}

/// Профиль локации из подписки.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub server: String,
    pub port: u16,
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub flow: Option<String>,
    pub tls: TlsConfig,
    pub transport: Transport,
    /// Исходный JSON профиля — только в кэше, не в экспортируемой диагностике.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raw: Option<serde_json::Value>,
}

/// Платформа, для которой задано правило.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Macos,
    Windows,
    Linux,
}

/// Правило allowlist: какой процесс пускать через прокси.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process_name: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_regex: Vec<String>,
    pub enabled: bool,
}

/// Режим DNS (PRD §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DnsMode {
    /// Выбранные процессы → remote, остальные → local.
    #[serde(rename = "process")]
    ProcessSplit,
    /// Системный DNS для всех (fallback с предупреждением).
    #[serde(rename = "system")]
    System,
}

/// Уровень логирования.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Пользовательские настройки, влияющие на генерацию конфига.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub mtu: u16,
    pub ipv6_enabled: bool,
    pub mixed_inbound_enabled: bool,
    pub mixed_port: u16,
    pub dns_mode: DnsMode,
    pub log_level: LogLevel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mtu: 1500,
            ipv6_enabled: false,
            mixed_inbound_enabled: true,
            mixed_port: 10808,
            dns_mode: DnsMode::ProcessSplit,
            log_level: LogLevel::Info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> Profile {
        Profile {
            id: "profile-1".to_string(),
            name: "Тестовая локация A".to_string(),
            protocol: Protocol::Vless,
            server: "example.com".to_string(),
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

    #[test]
    fn settings_defaults_match_prd() {
        let settings = Settings::default();
        assert_eq!(settings.mtu, 1500);
        assert!(!settings.ipv6_enabled);
        assert!(settings.mixed_inbound_enabled);
        assert_eq!(settings.mixed_port, 10808);
        assert_eq!(settings.dns_mode, DnsMode::ProcessSplit);
        assert_eq!(settings.log_level, LogLevel::Info);
    }

    #[test]
    fn dns_mode_serializes_as_documented() {
        assert_eq!(
            serde_json::to_value(DnsMode::ProcessSplit).unwrap(),
            "process"
        );
        assert_eq!(serde_json::to_value(DnsMode::System).unwrap(), "system");
    }

    #[test]
    fn profile_round_trip() {
        let profile = sample_profile();
        let encoded = serde_json::to_string(&profile).unwrap();
        let decoded: Profile = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, profile);
    }

    #[test]
    fn app_rule_with_path_regex_round_trip() {
        let rule = AppRule {
            id: "rule-chrome".to_string(),
            name: "Google Chrome".to_string(),
            platform: Platform::Macos,
            process_name: vec!["Google Chrome".to_string()],
            process_path: Vec::new(),
            path_regex: vec![r"^/Applications/Google Chrome\.app/".to_string()],
            enabled: true,
        };
        let encoded = serde_json::to_string(&rule).unwrap();
        let decoded: AppRule = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, rule);
        assert!(!encoded.contains("process_path"));
    }

    #[test]
    fn settings_round_trip() {
        let settings = Settings::default();
        let encoded = serde_json::to_string(&settings).unwrap();
        let decoded: Settings = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, settings);
    }
}
