//! Пользовательское состояние SplitRoute: настройки, профили, правила.
//!
//! Хранится в памяти и в файле пользовательского каталога (права `0600` на Unix).
//! Секреты (URL подписки, HWID) не пишутся в логи и не попадают в диагностику.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use splitroute_protocol::{AppRule, Language, Profile, Settings};

/// Настройки подписки и идентификации устройства.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSettings {
    /// Пустая строка означает «подписка не добавлена».
    pub url: String,
    pub hwid: String,
    pub user_agent: String,
    pub auto_update_hours: u16,
}

/// Поведение приложения и системная интеграция.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPreferences {
    pub launch_at_login: bool,
    pub launch_minimized: bool,
    pub connect_on_start: bool,
    pub close_to_tray: bool,
    /// Старые state.json не содержат поля — читаются как `ru`.
    #[serde(default)]
    pub language: Language,
}

/// Полные настройки приложения для UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    pub subscription: SubscriptionSettings,
    pub network: Settings,
    pub preferences: AppPreferences,
}

/// Собственный User-Agent приложения. Провайдеры отдают для него
/// base64-список `vless://`-ссылок; Happ-JSON тоже поддерживается (см.
/// `subscription::parse_subscription_report`), поэтому сохранённые значения
/// не мигрируют и не перезаписываются.
pub const DEFAULT_USER_AGENT: &str = "SplitRoute/0.1.0";

/// 16 hex-символов из CSPRNG. Не выводится из аппаратных идентификаторов.
pub fn generate_hwid() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(16)
        .collect()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            subscription: SubscriptionSettings {
                url: String::new(),
                hwid: generate_hwid(),
                user_agent: DEFAULT_USER_AGENT.to_string(),
                auto_update_hours: 12,
            },
            network: Settings::default(),
            preferences: AppPreferences {
                launch_at_login: false,
                launch_minimized: false,
                connect_on_start: false,
                close_to_tray: true,
                language: Language::Ru,
            },
        }
    }
}

/// Всё, что сохраняется на диск.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredData {
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub selected_profile_id: Option<String>,
    #[serde(default)]
    pub rules: Vec<AppRule>,
    /// Время последнего успешного обновления подписки (unix, секунды).
    #[serde(default)]
    pub profiles_fetched_at: Option<u64>,
}

/// Ошибка чтения/записи состояния.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("не удалось прочитать состояние: {0}")]
    Read(String),
    #[error("не удалось сохранить состояние: {0}")]
    Write(String),
    #[error("некорректный HWID: ожидается 16 hex-символов")]
    InvalidHwid,
}

/// Путь по умолчанию: `<config>/splitroute/state.json`.
pub fn default_data_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("splitroute")
        .join("state.json")
}

/// Хранилище состояния с сериализацией доступа.
pub struct Store {
    inner: Mutex<StoredData>,
    path: PathBuf,
}

impl Store {
    /// Загружает состояние из файла или создаёт значения по умолчанию.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = read_data(&path).unwrap_or_default();
        Self {
            inner: Mutex::new(data),
            path,
        }
    }

    pub fn default_location() -> Self {
        Self::load(default_data_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn data(&self) -> StoredData {
        self.inner.lock().unwrap().clone()
    }

    pub fn settings(&self) -> AppSettings {
        self.inner.lock().unwrap().settings.clone()
    }

    /// Сохраняет настройки. HWID проверяется перед записью.
    pub fn update_settings(&self, settings: AppSettings) -> Result<AppSettings, StoreError> {
        if settings.subscription.hwid.len() != 16
            || !settings
                .subscription
                .hwid
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(StoreError::InvalidHwid);
        }
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            data.settings = settings;
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(snapshot.settings)
    }

    /// Кладёт результат успешного обновления подписки (кэш последнего ответа).
    pub fn set_profiles(&self, profiles: Vec<Profile>, fetched_at: u64) -> Result<(), StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            if data
                .selected_profile_id
                .as_ref()
                .is_some_and(|id| !profiles.iter().any(|profile| &profile.id == id))
            {
                data.selected_profile_id = None;
            }
            if data.selected_profile_id.is_none() {
                data.selected_profile_id = profiles.first().map(|profile| profile.id.clone());
            }
            data.profiles = profiles;
            data.profiles_fetched_at = Some(fetched_at);
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))
    }

    pub fn select_profile(&self, id: &str) -> Result<(), StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            if !data.profiles.iter().any(|profile| profile.id == id) {
                return Err(StoreError::Read("профиль не найден в подписке".to_string()));
            }
            data.selected_profile_id = Some(id.to_string());
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))
    }

    pub fn rules(&self) -> Vec<AppRule> {
        self.inner.lock().unwrap().rules.clone()
    }

    /// Добавляет правило или заменяет существующее с тем же `id`.
    pub fn upsert_rule(&self, rule: AppRule) -> Result<Vec<AppRule>, StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            match data.rules.iter_mut().find(|item| item.id == rule.id) {
                Some(existing) => *existing = rule,
                None => data.rules.push(rule),
            }
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(snapshot.rules)
    }

    pub fn delete_rule(&self, id: &str) -> Result<Vec<AppRule>, StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            data.rules.retain(|rule| rule.id != id);
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(snapshot.rules)
    }
}

fn read_data(path: &Path) -> Option<StoredData> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_data(path: &Path, data: &StoredData) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, encoded)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(temp, path)
}

/// Текущее время в секундах с Unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use splitroute_protocol::{Platform, Protocol, TlsConfig, TlsKind, Transport};

    fn temp_path(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sr-store-{tag}-{}-{unique}.json",
            std::process::id()
        ))
    }

    fn profile(id: &str) -> Profile {
        Profile {
            id: id.to_string(),
            name: "Локация".to_string(),
            protocol: Protocol::Vless,
            server: "example.com".to_string(),
            port: 443,
            uuid: "d811a5ee-1d55-4dca-bd7f-f68b768a2241".to_string(),
            flow: None,
            tls: TlsConfig {
                kind: TlsKind::Reality,
                server_name: "example.com".to_string(),
                public_key: Some("key".to_string()),
                short_id: None,
                fingerprint: None,
                alpn: Vec::new(),
            },
            transport: Transport::Tcp,
            raw: None,
        }
    }

    fn rule(id: &str, enabled: bool) -> AppRule {
        AppRule {
            id: id.to_string(),
            name: "Chrome".to_string(),
            platform: Platform::Macos,
            process_name: vec!["Google Chrome".to_string()],
            process_path: Vec::new(),
            path_regex: Vec::new(),
            enabled,
        }
    }

    #[test]
    fn defaults_are_generated_with_valid_hwid() {
        let store = Store::load(temp_path("defaults"));
        let settings = store.settings();
        assert_eq!(settings.subscription.hwid.len(), 16);
        assert!(settings
            .subscription
            .hwid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
        assert!(settings.subscription.url.is_empty());
        assert_eq!(
            settings.subscription.user_agent, "SplitRoute/0.1.0",
            "собственный UA: провайдер отдаёт base64-список vless-ссылок"
        );
        assert!(settings.preferences.close_to_tray);
        assert_eq!(settings.preferences.language, Language::Ru);
        assert!(store.data().profiles.is_empty());
    }

    /// Старый state.json без поля `language` читается как `ru`.
    #[test]
    fn legacy_preferences_without_language_read_as_ru() {
        let raw = r#"{
            "launch_at_login": true,
            "launch_minimized": false,
            "connect_on_start": false,
            "close_to_tray": true
        }"#;
        let preferences: AppPreferences = serde_json::from_str(raw).unwrap();
        assert_eq!(preferences.language, Language::Ru);
    }

    #[test]
    fn language_is_saved_and_reloaded() {
        let path = temp_path("language");
        let store = Store::load(&path);
        let mut settings = store.settings();
        settings.preferences.language = Language::En;
        store.update_settings(settings).unwrap();

        let reloaded = Store::load(&path);
        assert_eq!(reloaded.settings().preferences.language, Language::En);
        let _ = fs::remove_file(&path);
    }

    /// Сохранённый UA — это выбор пользователя: загрузка его не переписывает.
    #[test]
    fn saved_user_agent_is_kept_on_load() {
        let path = temp_path("saved-ua");
        let mut data = StoredData::default();
        data.settings.subscription.user_agent = "Custom/1.0".to_string();
        write_data(&path, &data).unwrap();

        let store = Store::load(&path);
        assert_eq!(store.settings().subscription.user_agent, "Custom/1.0");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn settings_round_trip_through_file() {
        let path = temp_path("settings");
        let store = Store::load(&path);
        let mut settings = store.settings();
        settings.subscription.url = "https://example.invalid/sub".to_string();
        settings.network.mtu = 1400;
        store.update_settings(settings.clone()).unwrap();

        let reloaded = Store::load(&path);
        assert_eq!(reloaded.settings(), settings);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn invalid_hwid_is_rejected() {
        let store = Store::load(temp_path("hwid"));
        let mut settings = store.settings();
        settings.subscription.hwid = "zz".to_string();
        assert!(matches!(
            store.update_settings(settings),
            Err(StoreError::InvalidHwid)
        ));
    }

    #[test]
    fn profiles_are_cached_and_selection_reset_when_missing() {
        let store = Store::load(temp_path("profiles"));
        store
            .set_profiles(vec![profile("p1"), profile("p2")], 123)
            .unwrap();
        store.select_profile("p2").unwrap();
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p2"));

        store.set_profiles(vec![profile("p1")], 124).unwrap();
        let data = store.data();
        assert_eq!(data.profiles.len(), 1);
        assert_eq!(data.selected_profile_id.as_deref(), Some("p1"));
        assert_eq!(data.profiles_fetched_at, Some(124));
    }

    #[test]
    fn select_unknown_profile_is_error() {
        let store = Store::load(temp_path("select"));
        assert!(store.select_profile("missing").is_err());
    }

    #[test]
    fn rules_upsert_and_delete() {
        let store = Store::load(temp_path("rules"));
        store.upsert_rule(rule("r1", true)).unwrap();
        let rules = store.upsert_rule(rule("r1", false)).unwrap();
        assert_eq!(rules.len(), 1);
        assert!(!rules[0].enabled);

        store.upsert_rule(rule("r2", true)).unwrap();
        let rules = store.delete_rule("r1").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "r2");
    }
}
