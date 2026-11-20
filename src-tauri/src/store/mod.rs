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

/// Глобальные настройки подписок: HWID — идентификатор устройства, один на все
/// провайдера. Параметры конкретной подписки (URL, UA, интервал) живут в
/// `SubscriptionState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSettings {
    pub hwid: String,
}

/// Одна подписка: параметры провайдера и кэш последнего успешного ответа.
/// Несколько подписок независимы; активная локация — одна на приложение.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionState {
    /// Стабильный идентификатор записи; генерируется при создании.
    pub id: String,
    /// Отображаемое имя. Пустое — UI показывает «Подписка N».
    pub name: String,
    pub url: String,
    pub user_agent: String,
    pub auto_update_hours: u16,
    /// Кэш последнего успешного ответа; при ошибке сети сохраняется.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Время последнего успешного обновления (unix, секунды).
    #[serde(default)]
    pub fetched_at: Option<u64>,
}

/// Порог деградации по умолчанию: новая локация должна быть быстрее на 100 мс.
pub const DEFAULT_DEGRADATION_MS: u16 = 100;
/// Интервал фоновой проверки задержек, секунды.
pub const DEFAULT_INTERVAL_SECS: u32 = 60;
/// Сколько подряд проверок должна длиться деградация.
pub const DEFAULT_CONFIRMATIONS: u8 = 2;
/// Сколько подряд неудачных замеров текущей локации достаточно для переключения.
pub const DEFAULT_FAILURES_TO_SWITCH: u8 = 2;

/// Автовыбор и автопереключение локации (PRD §6.1, §6.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoSelectSettings {
    /// Автовыбор лучшей локации после ручной проверки задержек.
    #[serde(default)]
    pub enabled: bool,
    /// Автопереключение при деградации во время активного подключения.
    #[serde(default)]
    pub auto_switch: bool,
    /// Минимальное улучшение задержки, мс.
    #[serde(default = "default_degradation_ms")]
    pub degradation_ms: u16,
    /// Интервал проверки, секунды.
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u32,
    /// Сколько проверок подряд подтверждают деградацию.
    #[serde(default = "default_confirmations")]
    pub confirmations: u8,
    /// Сколько неудачных замеров подряд у текущей локации достаточно.
    #[serde(default = "default_failures_to_switch")]
    pub failures_to_switch: u8,
}

fn default_degradation_ms() -> u16 {
    DEFAULT_DEGRADATION_MS
}

fn default_interval_secs() -> u32 {
    DEFAULT_INTERVAL_SECS
}

fn default_confirmations() -> u8 {
    DEFAULT_CONFIRMATIONS
}

fn default_failures_to_switch() -> u8 {
    DEFAULT_FAILURES_TO_SWITCH
}

impl Default for AutoSelectSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_switch: false,
            degradation_ms: DEFAULT_DEGRADATION_MS,
            interval_secs: DEFAULT_INTERVAL_SECS,
            confirmations: DEFAULT_CONFIRMATIONS,
            failures_to_switch: DEFAULT_FAILURES_TO_SWITCH,
        }
    }
}

impl AutoSelectSettings {
    /// Приводит значения к допустимым диапазонам: ручная правка state.json и
    /// старые записи не должны ломать фоновые проверки.
    pub fn normalized(mut self) -> Self {
        self.degradation_ms = self.degradation_ms.min(2000);
        self.interval_secs = self.interval_secs.clamp(15, 3600);
        self.confirmations = self.confirmations.clamp(1, 5);
        self.failures_to_switch = self.failures_to_switch.clamp(1, 5);
        self
    }
}

/// Канал обновлений. Неизвестное значение в state.json читается как `stable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
}

impl<'de> Deserialize<'de> for UpdateChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.trim().to_ascii_lowercase().as_str() {
            "beta" => Self::Beta,
            _ => Self::Stable,
        })
    }
}

/// Настройки автообновления приложения (S4-T8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSettings {
    /// Проверять обновления при запуске. Установка всегда требует явного
    /// действия пользователя.
    #[serde(default)]
    pub check_on_start: bool,
    /// Канал обновлений: `stable` или `beta`.
    #[serde(default)]
    pub channel: UpdateChannel,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_on_start: false,
            channel: UpdateChannel::Stable,
        }
    }
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
    /// Старые state.json не содержат блока — читаются с выключенными функциями.
    #[serde(default)]
    pub auto_select: AutoSelectSettings,
    /// Старые state.json не содержат блока — проверка выключена, канал stable.
    #[serde(default)]
    pub updates: UpdateSettings,
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
                hwid: generate_hwid(),
            },
            network: Settings::default(),
            preferences: AppPreferences {
                launch_at_login: false,
                launch_minimized: false,
                connect_on_start: false,
                close_to_tray: true,
                language: Language::Ru,
                auto_select: AutoSelectSettings::default(),
                updates: UpdateSettings::default(),
            },
        }
    }
}

/// Всё, что сохраняется на диск.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredData {
    #[serde(default)]
    pub settings: AppSettings,
    /// Подписки в порядке добавления. Устаревший файл с одной подпиской
    /// переносится в первую запись при чтении (см. `StoredData::migrate_legacy`).
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionState>,
    /// Активная локация — одна на приложение. Идентификатор профиля
    /// вычисляется из содержимого, поэтому коллизия между подписками
    /// означает буквально одинаковые профили.
    #[serde(default)]
    pub selected_profile_id: Option<String>,
    #[serde(default)]
    pub rules: Vec<AppRule>,
}

impl StoredData {
    /// Все профили всех подписок в порядке добавления подписок.
    pub fn all_profiles(&self) -> Vec<Profile> {
        self.subscriptions
            .iter()
            .flat_map(|subscription| subscription.profiles.iter().cloned())
            .collect()
    }

    /// Ищет профиль по идентификатору среди всех подписок.
    pub fn find_profile(&self, id: &str) -> Option<&Profile> {
        self.subscriptions
            .iter()
            .flat_map(|subscription| subscription.profiles.iter())
            .find(|profile| profile.id == id)
    }

    /// Есть ли профиль с таким идентификатором в какой-нибудь подписке.
    fn profile_exists(&self, id: &str) -> bool {
        self.find_profile(id).is_some()
    }

    /// Сбрасывает выбор, если выбранного профиля больше нет ни в одной подписке.
    /// Автовыбор здесь не выполняется: он происходит только при успешной
    /// загрузке подписки.
    fn reconcile_selection(&mut self) {
        if let Some(selected) = self.selected_profile_id.as_ref() {
            if !self.profile_exists(selected) {
                self.selected_profile_id = None;
            }
        }
    }

    /// Переносит состояние до появления нескольких подписок: один URL в
    /// `settings.subscription` и общий список профилей становятся первой
    /// записью. Возвращает `true`, если миграция выполнилась.
    fn migrate_legacy(&mut self, raw: &serde_json::Value) -> bool {
        if !self.subscriptions.is_empty() {
            return false;
        }
        let Some(legacy) = raw
            .get("settings")
            .and_then(|settings| settings.get("subscription"))
        else {
            return false;
        };
        let url = legacy
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            return false;
        }
        let user_agent = legacy
            .get("user_agent")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let auto_update_hours = legacy
            .get("auto_update_hours")
            .and_then(|value| value.as_u64())
            .map(|value| value as u16)
            .unwrap_or(12);
        let profiles: Vec<Profile> = raw
            .get("profiles")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        let fetched_at = raw
            .get("profiles_fetched_at")
            .and_then(|value| value.as_u64());
        self.subscriptions.push(SubscriptionState {
            id: new_subscription_id(),
            name: String::new(),
            url,
            user_agent,
            auto_update_hours,
            profiles,
            fetched_at,
        });
        true
    }
}

/// Стабильный идентификатор записи подписки.
pub fn new_subscription_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
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
    #[error("{0}")]
    NotFound(String),
    #[error("URL подписки не задан")]
    InvalidUrl,
}

/// Путь по умолчанию: `<config>/splitroute/state.json`.
pub fn default_data_path() -> PathBuf {
    // Dev-проверки (research §31): изолированный стор без переопределения
    // `HOME` — оно ломает данные webview, окно открывается пустым.
    if let Ok(path) = std::env::var("SPLITROUTE_STATE") {
        return PathBuf::from(path);
    }
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

    /// Сохраняет настройки. HWID проверяется перед записью, диапазоны
    /// автовыбора приводятся к допустимым значениям.
    pub fn update_settings(&self, settings: AppSettings) -> Result<AppSettings, StoreError> {
        let mut settings = settings;
        settings.preferences.auto_select = settings.preferences.auto_select.normalized();
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

    /// Список подписок в порядке добавления.
    pub fn subscriptions(&self) -> Vec<SubscriptionState> {
        self.inner.lock().unwrap().subscriptions.clone()
    }

    /// Одна подписка по идентификатору.
    pub fn subscription(&self, id: &str) -> Option<SubscriptionState> {
        self.inner
            .lock()
            .unwrap()
            .subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
            .cloned()
    }

    /// Добавляет подписку. Пустой UA заменяется на собственный приложения.
    pub fn add_subscription(
        &self,
        name: &str,
        url: &str,
        user_agent: &str,
        auto_update_hours: u16,
    ) -> Result<SubscriptionState, StoreError> {
        if url.trim().is_empty() {
            return Err(StoreError::InvalidUrl);
        }
        let entry = SubscriptionState {
            id: new_subscription_id(),
            name: name.trim().to_string(),
            url: url.trim().to_string(),
            user_agent: {
                let trimmed = user_agent.trim();
                if trimmed.is_empty() {
                    DEFAULT_USER_AGENT.to_string()
                } else {
                    trimmed.to_string()
                }
            },
            auto_update_hours,
            profiles: Vec::new(),
            fetched_at: None,
        };
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            data.subscriptions.push(entry.clone());
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(entry)
    }

    /// Обновляет параметры подписки; кэш профилей сохраняется, если URL не
    /// изменился. Смена URL сбрасывает кэш прежнего провайдера.
    pub fn update_subscription(
        &self,
        id: &str,
        name: &str,
        url: &str,
        user_agent: &str,
        auto_update_hours: u16,
    ) -> Result<SubscriptionState, StoreError> {
        if url.trim().is_empty() {
            return Err(StoreError::InvalidUrl);
        }
        let url = url.trim().to_string();
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            let entry = data
                .subscriptions
                .iter_mut()
                .find(|subscription| subscription.id == id)
                .ok_or_else(|| StoreError::NotFound("подписка не найдена".to_string()))?;
            entry.name = name.trim().to_string();
            let url_changed = entry.url != url;
            entry.url = url;
            let trimmed = user_agent.trim();
            entry.user_agent = if trimmed.is_empty() {
                DEFAULT_USER_AGENT.to_string()
            } else {
                trimmed.to_string()
            };
            entry.auto_update_hours = auto_update_hours;
            if url_changed {
                // Кэш относится к прежнему провайдеру: показать его локации
                // для нового URL было бы обманом.
                entry.profiles.clear();
                entry.fetched_at = None;
            }
            data.reconcile_selection();
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))?;
        self.subscription(id)
            .ok_or_else(|| StoreError::NotFound("подписка не найдена".to_string()))
    }

    /// Удаляет подписку вместе с её кэшем. Активный туннель не затрагивается;
    /// выбор локации сбрасывается, только если профиль исчез отовсюду.
    pub fn delete_subscription(&self, id: &str) -> Result<Vec<SubscriptionState>, StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            if !data
                .subscriptions
                .iter()
                .any(|subscription| subscription.id == id)
            {
                return Err(StoreError::NotFound("подписка не найдена".to_string()));
            }
            data.subscriptions
                .retain(|subscription| subscription.id != id);
            data.reconcile_selection();
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(snapshot.subscriptions)
    }

    /// Кладёт результат успешного обновления одной подписки (кэш последнего
    /// ответа). Выбор локации автоматически переходит на первый профиль этой
    /// подписки, только если прежний выбор исчез отовсюду.
    pub fn set_subscription_profiles(
        &self,
        id: &str,
        profiles: Vec<Profile>,
        fetched_at: u64,
    ) -> Result<(), StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            let index = data
                .subscriptions
                .iter()
                .position(|subscription| subscription.id == id)
                .ok_or_else(|| StoreError::NotFound("подписка не найдена".to_string()))?;
            {
                let entry = &mut data.subscriptions[index];
                entry.profiles = profiles;
                entry.fetched_at = Some(fetched_at);
            }
            // Существование выбранного профиля проверяется уже по новому
            // состоянию: пропавший профиль мог быть именно в этой подписке.
            let selection_missing = match data.selected_profile_id.as_ref() {
                Some(selected) => !data.profile_exists(selected),
                None => true,
            };
            if selection_missing {
                data.selected_profile_id = data.subscriptions[index]
                    .profiles
                    .first()
                    .map(|profile| profile.id.clone());
            }
            data.clone()
        };
        write_data(&self.path, &snapshot).map_err(|error| StoreError::Write(error.to_string()))
    }

    pub fn select_profile(&self, id: &str) -> Result<(), StoreError> {
        let snapshot = {
            let mut data = self.inner.lock().unwrap();
            if !data.profile_exists(id) {
                return Err(StoreError::Read(
                    "профиль не найден ни в одной подписке".to_string(),
                ));
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
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let mut data: StoredData = serde_json::from_value(value.clone()).ok()?;
    if data.migrate_legacy(&value) {
        // Миграцию сразу пишем на диск: идентификатор записи подписки не
        // должен меняться между запусками. Ошибка записи не критична — при
        // следующем запуске миграция повторится.
        let _ = write_data(path, &data);
    }
    Some(data)
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

    /// `SPLITROUTE_STATE` перенаправляет стор (dev-проверки с изолированным
    /// состоянием). Переменную ставит и снимает сам тест: гонки с параллельными
    /// тестами нет — `default_data_path` они не вызывают.
    #[test]
    fn state_path_env_override_redirects_store() {
        let path = temp_path("env-override");
        // SAFETY: тесты исполняются в разных потоках, но этот env читается
        // только здесь и в `default_data_path` вне тестов.
        unsafe { std::env::set_var("SPLITROUTE_STATE", &path) };
        assert_eq!(default_data_path(), path);
        unsafe { std::env::remove_var("SPLITROUTE_STATE") };
        assert_ne!(default_data_path(), path);
        let _ = std::fs::remove_file(&path);
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
        assert!(
            store.data().subscriptions.is_empty(),
            "свежая установка не содержит подписок"
        );
        assert!(settings.preferences.close_to_tray);
        assert_eq!(settings.preferences.language, Language::Ru);
        assert!(store.data().all_profiles().is_empty());
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

    /// Старый state.json без блока автовыбора читается с безопасными значениями.
    #[test]
    fn legacy_preferences_without_auto_select_read_as_disabled() {
        let raw = r#"{
            "launch_at_login": true,
            "launch_minimized": false,
            "connect_on_start": false,
            "close_to_tray": true
        }"#;
        let preferences: AppPreferences = serde_json::from_str(raw).unwrap();
        let auto = preferences.auto_select;
        assert!(!auto.enabled);
        assert!(!auto.auto_switch);
        assert_eq!(auto.degradation_ms, DEFAULT_DEGRADATION_MS);
        assert_eq!(auto.interval_secs, DEFAULT_INTERVAL_SECS);
        assert_eq!(auto.confirmations, DEFAULT_CONFIRMATIONS);
        assert_eq!(auto.failures_to_switch, DEFAULT_FAILURES_TO_SWITCH);
    }

    #[test]
    fn auto_select_values_are_normalized_on_save() {
        let store = Store::load(temp_path("auto-select"));
        let mut settings = store.settings();
        settings.preferences.auto_select = AutoSelectSettings {
            enabled: true,
            auto_switch: true,
            degradation_ms: 9000,
            interval_secs: 1,
            confirmations: 0,
            failures_to_switch: 9,
        };
        let saved = store.update_settings(settings).unwrap();
        assert_eq!(saved.preferences.auto_select.degradation_ms, 2000);
        assert_eq!(saved.preferences.auto_select.interval_secs, 15);
        assert_eq!(saved.preferences.auto_select.confirmations, 1);
        assert_eq!(saved.preferences.auto_select.failures_to_switch, 5);
    }

    /// Старый state.json без блока обновлений: проверка выключена, канал stable.
    #[test]
    fn legacy_preferences_without_updates_read_as_disabled_stable() {
        let raw = r#"{
            "launch_at_login": true,
            "launch_minimized": false,
            "connect_on_start": false,
            "close_to_tray": true
        }"#;
        let preferences: AppPreferences = serde_json::from_str(raw).unwrap();
        assert!(!preferences.updates.check_on_start);
        assert_eq!(preferences.updates.channel, UpdateChannel::Stable);
    }

    /// Неизвестный или испорченный канал не должен ломать чтение настроек.
    #[test]
    fn unknown_update_channel_normalizes_to_stable() {
        for value in ["", "nightly", "STABLE", "  stable  "] {
            let raw = format!(r#"{{"check_on_start": true, "channel": "{value}"}}"#);
            let updates: UpdateSettings = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                updates.channel,
                UpdateChannel::Stable,
                "значение {value:?} читается как stable"
            );
            assert!(updates.check_on_start);
        }
        let beta: UpdateSettings = serde_json::from_str(r#"{"channel":"BETA"}"#).unwrap();
        assert_eq!(beta.channel, UpdateChannel::Beta);
    }

    #[test]
    fn update_channel_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(UpdateChannel::Stable).unwrap(),
            "stable"
        );
        assert_eq!(serde_json::to_value(UpdateChannel::Beta).unwrap(), "beta");
    }

    #[test]
    fn update_settings_round_trip_through_file() {
        let path = temp_path("updates");
        let store = Store::load(&path);
        let mut settings = store.settings();
        settings.preferences.updates = UpdateSettings {
            check_on_start: true,
            channel: UpdateChannel::Beta,
        };
        store.update_settings(settings.clone()).unwrap();

        let reloaded = Store::load(&path);
        assert_eq!(
            reloaded.settings().preferences.updates,
            settings.preferences.updates
        );
        let _ = fs::remove_file(&path);
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

    /// Сохранённый UA подписки — это выбор пользователя: загрузка его
    /// не переписывает.
    #[test]
    fn saved_subscription_user_agent_is_kept_on_load() {
        let path = temp_path("saved-ua");
        let store = Store::load(&path);
        let entry = store
            .add_subscription("", "https://example.invalid/sub", "Custom/1.0", 12)
            .unwrap();
        write_data(&path, &store.data()).unwrap();

        let reloaded = Store::load(&path);
        assert_eq!(
            reloaded.subscription(&entry.id).unwrap().user_agent,
            "Custom/1.0"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn settings_round_trip_through_file() {
        let path = temp_path("settings");
        let store = Store::load(&path);
        let mut settings = store.settings();
        settings.network.mtu = 1400;
        store.update_settings(settings.clone()).unwrap();
        store
            .add_subscription("Основная", "https://example.invalid/sub", "", 6)
            .unwrap();

        let reloaded = Store::load(&path);
        assert_eq!(reloaded.settings(), settings);
        let subscriptions = reloaded.subscriptions();
        assert_eq!(subscriptions.len(), 1);
        assert_eq!(subscriptions[0].name, "Основная");
        assert_eq!(subscriptions[0].url, "https://example.invalid/sub");
        assert_eq!(subscriptions[0].user_agent, DEFAULT_USER_AGENT);
        assert_eq!(subscriptions[0].auto_update_hours, 6);
        let _ = fs::remove_file(&path);
    }

    /// Устаревший state.json с одной подпиской переносится в первую запись,
    /// включая кэш профилей и время последнего ответа.
    #[test]
    fn legacy_state_migrates_to_first_subscription() {
        let path = temp_path("legacy");
        let mut data = StoredData::default();
        data.settings.subscription.hwid = "0123456789abcdef".to_string();
        write_data(&path, &data).unwrap();
        // Правим файл руками: в новой модели этих полей больше нет.
        let raw = fs::read_to_string(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        value["settings"]["subscription"] = serde_json::json!({
            "url": "https://example.invalid/sub",
            "hwid": "0123456789abcdef",
            "user_agent": "Custom/1.0",
            "auto_update_hours": 6,
        });
        value["profiles"] = serde_json::to_value(vec![profile("p1"), profile("p2")]).unwrap();
        value["selected_profile_id"] = serde_json::json!("p2");
        value["profiles_fetched_at"] = serde_json::json!(1234);
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

        let store = Store::load(&path);
        let subscriptions = store.subscriptions();
        assert_eq!(subscriptions.len(), 1, "создана одна запись");
        let entry = &subscriptions[0];
        assert!(!entry.id.is_empty());
        assert_eq!(entry.url, "https://example.invalid/sub");
        assert_eq!(entry.user_agent, "Custom/1.0");
        assert_eq!(entry.auto_update_hours, 6);
        assert_eq!(entry.profiles.len(), 2);
        assert_eq!(entry.fetched_at, Some(1234));
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p2"));
        assert_eq!(
            store.settings().subscription.hwid,
            "0123456789abcdef",
            "HWID сохраняется"
        );

        // Миграция записана на диск: повторное чтение даёт те же идентификатор
        // и данные, а устаревшие поля больше не хранятся.
        let reloaded = Store::load(&path);
        assert_eq!(reloaded.subscriptions(), subscriptions);
        let raw = fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(value.get("profiles").is_none(), "плоский список удалён");
        assert!(
            value["settings"]["subscription"].get("url").is_none(),
            "URL теперь в записи подписки"
        );
        let _ = fs::remove_file(&path);
    }

    /// Файл без подписки (пустой URL) не создаёт записей.
    #[test]
    fn legacy_state_without_url_migrates_to_empty_list() {
        let path = temp_path("legacy-empty");
        let mut data = StoredData::default();
        data.settings.subscription.hwid = "0123456789abcdef".to_string();
        write_data(&path, &data).unwrap();

        let store = Store::load(&path);
        assert!(store.subscriptions().is_empty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn add_subscription_requires_url_and_fills_defaults() {
        let store = Store::load(temp_path("add"));
        assert!(matches!(
            store.add_subscription("", "  ", "", 12),
            Err(StoreError::InvalidUrl)
        ));
        let entry = store
            .add_subscription("  Главная  ", " url ", "", 0)
            .unwrap();
        assert_eq!(entry.name, "Главная", "имя обрезается");
        assert_eq!(entry.url, "url");
        assert_eq!(entry.user_agent, DEFAULT_USER_AGENT, "пустой UA — свой");
        assert_eq!(entry.auto_update_hours, 0);
        assert!(entry.profiles.is_empty());
        assert_eq!(entry.fetched_at, None);
    }

    #[test]
    fn subscription_update_keeps_cache_until_url_changes() {
        let store = Store::load(temp_path("update"));
        let entry = store
            .add_subscription("", "https://a.invalid/sub", "", 12)
            .unwrap();
        store
            .set_subscription_profiles(&entry.id, vec![profile("p1")], 100)
            .unwrap();
        store.select_profile("p1").unwrap();

        // UA и интервал меняются — кэш остаётся.
        let updated = store
            .update_subscription(&entry.id, "A", "https://a.invalid/sub", "Custom/2.0", 6)
            .unwrap();
        assert_eq!(updated.user_agent, "Custom/2.0");
        assert_eq!(updated.auto_update_hours, 6);
        assert_eq!(updated.profiles.len(), 1);
        assert_eq!(updated.fetched_at, Some(100));

        // Смена URL сбрасывает кэш и оставляет выбор пустым: профиль исчез.
        let updated = store
            .update_subscription(&entry.id, "A", "https://b.invalid/sub", "", 6)
            .unwrap();
        assert!(updated.profiles.is_empty());
        assert_eq!(updated.fetched_at, None);
        assert_eq!(store.data().selected_profile_id, None);
        assert_eq!(
            updated.user_agent, DEFAULT_USER_AGENT,
            "пустой UA заменяется на свой"
        );
        assert!(matches!(
            store.update_subscription(&entry.id, "", " ", "", 6),
            Err(StoreError::InvalidUrl)
        ));
    }

    #[test]
    fn subscription_delete_resets_selection_only_when_profile_gone() {
        let store = Store::load(temp_path("delete"));
        let first = store
            .add_subscription("A", "https://a.invalid/sub", "", 12)
            .unwrap();
        let second = store
            .add_subscription("B", "https://b.invalid/sub", "", 12)
            .unwrap();
        store
            .set_subscription_profiles(&first.id, vec![profile("p1")], 1)
            .unwrap();
        store
            .set_subscription_profiles(&second.id, vec![profile("p2")], 2)
            .unwrap();
        store.select_profile("p2").unwrap();

        // Удаление другой подписки выбор не трогает.
        let rest = store.delete_subscription(&first.id).unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p2"));

        // Удаление подписки с выбранной локацией сбрасывает выбор.
        store.delete_subscription(&second.id).unwrap();
        assert_eq!(store.data().selected_profile_id, None);
        assert!(matches!(
            store.delete_subscription(&first.id),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn profiles_are_cached_per_subscription_and_selection_survives_other_updates() {
        let store = Store::load(temp_path("profiles"));
        let first = store
            .add_subscription("A", "https://a.invalid/sub", "", 12)
            .unwrap();
        let second = store
            .add_subscription("B", "https://b.invalid/sub", "", 12)
            .unwrap();

        // Первая загрузка выбирает первый профиль автоматически.
        store
            .set_subscription_profiles(&first.id, vec![profile("p1"), profile("p2")], 123)
            .unwrap();
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p1"));
        store.select_profile("p2").unwrap();

        // Обновление второй подписки не трогает выбор из первой.
        store
            .set_subscription_profiles(&second.id, vec![profile("p3")], 124)
            .unwrap();
        let data = store.data();
        assert_eq!(data.subscriptions.len(), 2);
        assert!(data.find_profile("p3").is_some());
        assert_eq!(data.selected_profile_id.as_deref(), Some("p2"));
        assert_eq!(
            data.subscriptions
                .iter()
                .find(|subscription| subscription.id == second.id)
                .unwrap()
                .fetched_at,
            Some(124)
        );

        // Обновление первой подписки убирает p2: выбор переносится на первый
        // профиль обновлённой подписки.
        store
            .set_subscription_profiles(&first.id, vec![profile("p1")], 125)
            .unwrap();
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p1"));

        // Обновление второй подписки выбор не трогает: p1 живёт в первой.
        store
            .set_subscription_profiles(&second.id, vec![profile("p4")], 126)
            .unwrap();
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p1"));
    }

    #[test]
    fn select_unknown_profile_is_error() {
        let store = Store::load(temp_path("select"));
        assert!(store.select_profile("missing").is_err());
        let entry = store
            .add_subscription("", "https://a.invalid/sub", "", 12)
            .unwrap();
        store
            .set_subscription_profiles(&entry.id, vec![profile("p1")], 1)
            .unwrap();
        store.select_profile("p1").unwrap();
        assert_eq!(store.data().selected_profile_id.as_deref(), Some("p1"));
    }

    #[test]
    fn set_profiles_on_missing_subscription_is_not_found() {
        let store = Store::load(temp_path("missing-sub"));
        assert!(matches!(
            store.set_subscription_profiles("nope", vec![profile("p1")], 1),
            Err(StoreError::NotFound(_))
        ));
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
