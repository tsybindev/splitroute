//! Тонкие Tauri commands: без бизнес-логики в UI, только вызов ядра.
//!
//! UI не формирует конфиг sing-box: подключение собирается здесь из сохранённых
//! настроек, выбранного профиля и правил allowlist (PRD §6.2).

use std::sync::Arc;

use serde::Serialize;
use splitroute_protocol::{AppRule, HelperStatus, InstalledApp, LogLevel, Profile};
use tauri::Manager;

use crate::catalog;
use crate::config::{build_config, ConfigError};
use crate::logging::{LogBuffer, LogEntry};
use crate::privileged::{HelperClient, HelperClientError, HelperInstaller, InstallerError};
use crate::store::{now_secs, AppSettings, Store, StoreError};
use crate::subscription::{
    fetch_and_parse, FetchError, FetchRequest, SkippedProfile, SubscriptionFetcher,
};

/// Ошибка команды: стабильный код + русский текст для пользователя.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl From<FetchError> for CommandError {
    fn from(error: FetchError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<StoreError> for CommandError {
    fn from(error: StoreError) -> Self {
        Self::new("STORE", error.to_string())
    }
}

impl From<ConfigError> for CommandError {
    fn from(error: ConfigError) -> Self {
        Self::new("CONFIG", error.to_string())
    }
}

impl From<HelperClientError> for CommandError {
    fn from(error: HelperClientError) -> Self {
        let code = match error {
            HelperClientError::NotInstalled(_) => "HELPER_NOT_INSTALLED",
            HelperClientError::Unavailable(_) => "HELPER_UNAVAILABLE",
            HelperClientError::Timeout => "HELPER_TIMEOUT",
            HelperClientError::Protocol(_) => "HELPER_PROTOCOL",
            HelperClientError::Helper { .. } => "HELPER_ERROR",
            HelperClientError::InvalidPayload(_) => "HELPER_INVALID_RESPONSE",
        };
        Self::new(code, error.to_string())
    }
}

impl From<InstallerError> for CommandError {
    /// Код ошибки различает причину, чтобы UI показал её, а не общую строку.
    fn from(error: InstallerError) -> Self {
        let code = match &error {
            InstallerError::RequiresPrivileges => "INSTALLER_PRIVILEGES",
            InstallerError::Unsupported => "INSTALLER_UNSUPPORTED",
            InstallerError::MissingBinary(_) => "INSTALLER_MISSING_BINARY",
            InstallerError::Failed(_) => "INSTALLER_FAILED",
        };
        Self::new(code, error.to_string())
    }
}

/// Состояние приложения, доступное командам.
pub struct AppState {
    pub helper: Arc<dyn HelperClient>,
    pub installer: Arc<dyn HelperInstaller>,
    pub store: Store,
    pub fetcher: Arc<dyn SubscriptionFetcher>,
    pub logs: LogBuffer,
}

impl AppState {
    pub fn new(
        helper: Arc<dyn HelperClient>,
        installer: Arc<dyn HelperInstaller>,
        store: Store,
        fetcher: Arc<dyn SubscriptionFetcher>,
    ) -> Self {
        Self {
            helper,
            installer,
            store,
            fetcher,
            logs: LogBuffer::default(),
        }
    }
}

/// Профили и выбранная локация для экрана «Локации».
#[derive(Debug, Clone, Serialize)]
pub struct ProfilesResponse {
    pub profiles: Vec<Profile>,
    pub selected_profile_id: Option<String>,
    pub fetched_at: Option<u64>,
}

/// Результат обновления подписки.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionFetchResponse {
    pub profiles: Vec<Profile>,
    pub skipped: Vec<SkippedProfile>,
    pub selected_profile_id: Option<String>,
    pub fetched_at: Option<u64>,
}

fn profiles_response(store: &Store) -> ProfilesResponse {
    let data = store.data();
    ProfilesResponse {
        profiles: data.profiles.into_iter().map(sanitize_profile).collect(),
        selected_profile_id: data.selected_profile_id,
        fetched_at: data.profiles_fetched_at,
    }
}

/// Не отдаём UI исходный JSON профиля: в нём есть UUID и адрес сервера.
fn sanitize_profile(mut profile: Profile) -> Profile {
    profile.raw = None;
    profile
}

/// Собирает конфиг и запускает ядро через helper.
pub(crate) fn connect_inner(state: &AppState) -> Result<HelperStatus, CommandError> {
    let data = state.store.data();
    let selected = data.selected_profile_id.as_deref().ok_or_else(|| {
        CommandError::new(
            "NO_PROFILE",
            "Локация не выбрана. Обновите подписку и выберите локацию.",
        )
    })?;
    let profile = data
        .profiles
        .iter()
        .find(|profile| profile.id == selected)
        .ok_or_else(|| {
            CommandError::new(
                "PROFILE_MISSING",
                "Выбранная локация отсутствует в подписке.",
            )
        })?;

    let config = build_config(&data.settings.network, profile, &data.rules)?;
    match data.settings.network.dns_mode {
        splitroute_protocol::DnsMode::Proxy => {
            state
                .logs
                .push(LogLevel::Info, "DNS: режим proxy — весь DNS через прокси");
        }
        splitroute_protocol::DnsMode::System => {
            state.logs.push(
                LogLevel::Info,
                "DNS: режим system — DNS операционной системы",
            );
            state.logs.push(
                LogLevel::Warn,
                "DNS: режим system — DNS выбранных приложений уходит в локальный \
                 resolver, возможна утечка",
            );
        }
    }
    let status = state.helper.start(config)?;
    state
        .logs
        .push(LogLevel::Info, format!("Подключено: {}", profile.name));
    Ok(status)
}

pub(crate) fn disconnect_inner(state: &AppState) -> Result<HelperStatus, CommandError> {
    let status = state.helper.stop()?;
    state.logs.push(LogLevel::Info, "Подключение остановлено");
    Ok(status)
}

fn subscription_fetch_inner(
    state: &AppState,
    url: Option<String>,
    hwid: Option<String>,
    user_agent: Option<String>,
) -> Result<SubscriptionFetchResponse, CommandError> {
    let settings = state.store.settings();
    let url = url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.subscription.url);
    let hwid = hwid
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.subscription.hwid);
    let user_agent = user_agent
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.subscription.user_agent);

    let request = FetchRequest {
        url,
        hwid,
        user_agent,
    };
    if request.url.trim().is_empty() {
        return Err(FetchError::MissingUrl.into());
    }

    state.logs.push(LogLevel::Info, "Обновление подписки");
    match fetch_and_parse(state.fetcher.as_ref(), &request) {
        Ok(subscription) => {
            let fetched_at = now_secs();
            state
                .store
                .set_profiles(subscription.profiles.clone(), fetched_at)?;
            state.logs.push(
                LogLevel::Info,
                format!(
                    "Подписка обновлена: профилей {}",
                    subscription.profiles.len()
                ),
            );
            Ok(SubscriptionFetchResponse {
                profiles: subscription
                    .profiles
                    .into_iter()
                    .map(sanitize_profile)
                    .collect(),
                skipped: subscription.skipped,
                selected_profile_id: state.store.data().selected_profile_id,
                fetched_at: Some(fetched_at),
            })
        }
        Err(error) => {
            state
                .logs
                .push(LogLevel::Warn, format!("Ошибка подписки: {}", error.code()));
            Err(error.into())
        }
    }
}

fn settings_update_inner(
    state: &AppState,
    settings: AppSettings,
) -> Result<AppSettings, CommandError> {
    let saved = state.store.update_settings(settings)?;
    state.logs.push(LogLevel::Info, "Настройки сохранены");
    Ok(saved)
}

/// Диагностика без секретов: ни URL подписки, ни HWID, ни UUID.
fn diagnostics_text(state: &AppState) -> String {
    let data = state.store.data();
    let helper = match state.helper.status() {
        Ok(status) => format!(
            "{:?}{}",
            status.state,
            status
                .version
                .map(|version| format!(", {version}"))
                .unwrap_or_default()
        ),
        Err(error) => format!("недоступен ({})", error),
    };
    let location = data
        .selected_profile_id
        .as_ref()
        .and_then(|id| data.profiles.iter().find(|profile| &profile.id == id))
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| "не выбрана".to_string());
    let subscription = data
        .profiles_fetched_at
        .map(|time| format!("обновлена {time}"))
        .unwrap_or_else(|| "нет данных".to_string());

    format!(
        "SplitRoute {}\nОС: {} ({})\nHelper: {}\nЛокация: {}\nПодписка: {} (профилей {})\nПравил: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        helper,
        location,
        subscription,
        data.profiles.len(),
        data.rules.len()
    )
}

#[tauri::command]
pub fn helper_status(state: tauri::State<'_, AppState>) -> Result<HelperStatus, CommandError> {
    Ok(state.helper.status()?)
}

#[tauri::command]
pub fn connect(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<HelperStatus, CommandError> {
    let status = connect_inner(&state)?;
    crate::tray::publish(&app, &status);
    Ok(status)
}

#[tauri::command]
pub fn disconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<HelperStatus, CommandError> {
    let status = disconnect_inner(&state)?;
    crate::tray::publish(&app, &status);
    Ok(status)
}

#[tauri::command]
pub fn helper_install(state: tauri::State<'_, AppState>) -> Result<(), CommandError> {
    Ok(state.installer.install()?)
}

#[tauri::command]
pub fn helper_uninstall(state: tauri::State<'_, AppState>) -> Result<(), CommandError> {
    Ok(state.installer.uninstall()?)
}

/// `async` обязателен: запрос к провайдеру блокирующий и может идти десятки
/// секунд. В Tauri 2 синхронные команды выполняются на главном потоке, из-за
/// чего загрузка подписки замораживала всё окно до ответа или таймаута.
#[tauri::command(async)]
pub fn subscription_fetch(
    state: tauri::State<'_, AppState>,
    url: Option<String>,
    hwid: Option<String>,
    user_agent: Option<String>,
) -> Result<SubscriptionFetchResponse, CommandError> {
    subscription_fetch_inner(&state, url, hwid, user_agent)
}

#[tauri::command]
pub fn profiles_list(state: tauri::State<'_, AppState>) -> ProfilesResponse {
    profiles_response(&state.store)
}

#[tauri::command]
pub fn profile_select(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<ProfilesResponse, CommandError> {
    state.store.select_profile(&id)?;
    Ok(profiles_response(&state.store))
}

#[tauri::command]
pub fn rules_list(state: tauri::State<'_, AppState>) -> Vec<AppRule> {
    state.store.rules()
}

/// Каталог установленных приложений текущей ОС (PRD §6.3). Причины пропуска
/// битых записей уходят в лог, чтобы не теряться в отчёте сканирования.
#[tauri::command]
pub fn apps_installed(state: tauri::State<'_, AppState>) -> Vec<InstalledApp> {
    let report = catalog::scan();
    let found = report.apps.len();
    for reason in &report.skipped {
        state
            .logs
            .push(LogLevel::Debug, format!("Каталог: {reason}"));
    }
    state.logs.push(
        LogLevel::Info,
        format!("Каталог приложений: найдено {found}"),
    );
    report.apps
}

/// Готовит выключенное правило из записи каталога: macOS получает regex по
/// каталогу `.app`, Windows/Linux — полный путь к процессу (PRD §6.3).
#[tauri::command]
pub fn app_rule_from_installed(app: InstalledApp) -> AppRule {
    catalog::rule_from_app(&app)
}

/// Иконка приложения как data URL (S4-T2). `async`, чтобы чтение `.icns`/`.exe`
/// и декодирование PNG не выполнялись на главном потоке: в Tauri 2 синхронные
/// команды идут на main thread. `None` — штатный fallback без иконки.
#[tauri::command(async)]
pub fn app_icon(app: InstalledApp) -> Option<String> {
    catalog::icons::icon_for(&app)
}

#[tauri::command]
pub fn rules_upsert(
    state: tauri::State<'_, AppState>,
    rule: AppRule,
) -> Result<Vec<AppRule>, CommandError> {
    Ok(state.store.upsert_rule(rule)?)
}

#[tauri::command]
pub fn rules_delete(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<AppRule>, CommandError> {
    Ok(state.store.delete_rule(&id)?)
}

#[tauri::command]
pub fn settings_get(state: tauri::State<'_, AppState>) -> AppSettings {
    state.store.settings()
}

#[tauri::command]
pub fn settings_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, CommandError> {
    let saved = settings_update_inner(&state, settings)?;
    apply_autostart(&app, saved.preferences.launch_at_login);
    Ok(saved)
}

#[tauri::command]
pub fn hwid_generate() -> String {
    crate::store::generate_hwid()
}

#[tauri::command]
pub fn logs_list(
    state: tauri::State<'_, AppState>,
    level: Option<LogLevel>,
    limit: Option<usize>,
) -> Vec<LogEntry> {
    state.logs.list(level, limit.unwrap_or(200))
}

#[tauri::command]
pub fn diagnostics_get(state: tauri::State<'_, AppState>) -> String {
    diagnostics_text(&state)
}

/// Автозапуск применяется к системе как побочный эффект сохранения настройки.
/// Ошибка регистрации не отменяет сохранение: пользователь увидит предупреждение
/// в логах, а флаг останется сохранённым.
fn apply_autostart(app: &tauri::AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    let currently = autolaunch.is_enabled().unwrap_or(false);
    if currently == enabled {
        return;
    }
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    if let Err(error) = result {
        app.state::<AppState>()
            .logs
            .push(LogLevel::Warn, format!("Автозапуск: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privileged::{MockHelperClient, UnsupportedInstaller};
    use crate::store::{AppSettings, Store};
    use splitroute_protocol::{Platform, Settings};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct MockFetcher {
        body: Result<String, FetchError>,
    }

    impl SubscriptionFetcher for MockFetcher {
        fn fetch(&self, _request: &FetchRequest) -> Result<String, FetchError> {
            self.body.clone()
        }
    }

    /// Первый вызов успешен, последующие падают: проверка сохранения кэша.
    struct SwitchFetcher {
        calls: std::sync::atomic::AtomicUsize,
        body: String,
    }

    impl SubscriptionFetcher for SwitchFetcher {
        fn fetch(&self, _request: &FetchRequest) -> Result<String, FetchError> {
            if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(self.body.clone())
            } else {
                Err(FetchError::Network("нет сети".to_string()))
            }
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sr-cmd-{tag}-{}-{unique}.json", std::process::id()))
    }

    fn state_with(body: Result<String, FetchError>) -> AppState {
        AppState::new(
            Arc::new(MockHelperClient::new()),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("state")),
            Arc::new(MockFetcher { body }),
        )
    }

    fn fixture() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../spike/fixtures/subscription.json"
        ))
        .expect("фикстура подписки")
    }

    #[test]
    fn fetch_success_caches_profiles_and_selects_first() {
        let state = state_with(Ok(fixture()));
        let result = subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.profiles.len(), 2);
        assert_eq!(
            result.selected_profile_id.as_deref(),
            Some(result.profiles[0].id.as_str())
        );
        assert_eq!(state.store.data().profiles.len(), 2);
    }

    #[test]
    fn fetch_failure_keeps_cached_profiles() {
        let state = AppState::new(
            Arc::new(MockHelperClient::new()),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("cache")),
            Arc::new(SwitchFetcher {
                calls: std::sync::atomic::AtomicUsize::new(0),
                body: fixture(),
            }),
        );
        subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(state.store.data().profiles.len(), 2);

        let error = subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, "NETWORK");
        assert_eq!(
            state.store.data().profiles.len(),
            2,
            "кэш последнего успешного ответа сохраняется"
        );
    }

    #[test]
    fn responses_do_not_expose_raw_profile_json() {
        let state = state_with(Ok(fixture()));
        let response = subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        assert!(response
            .profiles
            .iter()
            .all(|profile| profile.raw.is_none()));
        assert!(profiles_response(&state.store)
            .profiles
            .iter()
            .all(|profile| profile.raw.is_none()));
        assert!(state
            .store
            .data()
            .profiles
            .iter()
            .all(|profile| profile.raw.is_some()));
    }

    #[test]
    fn fetch_without_url_is_typed_error() {
        let state = state_with(Ok(fixture()));
        let error = subscription_fetch_inner(&state, None, None, None).unwrap_err();
        assert_eq!(error.code, "MISSING_URL");
    }

    #[test]
    fn connect_without_profile_is_error() {
        let state = state_with(Ok(fixture()));
        let error = connect_inner(&state).unwrap_err();
        assert_eq!(error.code, "NO_PROFILE");
    }

    #[test]
    fn connect_starts_helper_with_generated_config() {
        let state = state_with(Ok(fixture()));
        subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        let status = connect_inner(&state).unwrap();
        assert_eq!(status.state, splitroute_protocol::CoreState::Running);
        assert_eq!(
            disconnect_inner(&state).unwrap().state,
            splitroute_protocol::CoreState::Stopped
        );
    }

    #[test]
    fn settings_update_rejects_invalid_hwid() {
        let state = state_with(Ok(fixture()));
        let mut settings: AppSettings = state.store.settings();
        settings.subscription.hwid = "not-hex".to_string();
        assert_eq!(
            settings_update_inner(&state, settings).unwrap_err().code,
            "STORE"
        );
    }

    #[test]
    fn diagnostics_contains_no_secrets() {
        let state = state_with(Ok(fixture()));
        let mut settings = state.store.settings();
        settings.subscription.url = "https://secret.invalid/sub".to_string();
        settings.subscription.hwid = "0123456789abcdef".to_string();
        settings_update_inner(&state, settings).unwrap();
        subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();

        let text = diagnostics_text(&state);
        assert!(!text.contains("secret.invalid"));
        assert!(!text.contains("0123456789abcdef"));
        assert!(!text.contains("d811a5ee"));
        assert!(text.contains("SplitRoute"));
    }

    #[test]
    fn rules_are_saved_and_listed() {
        let state = state_with(Ok(fixture()));
        let rule = AppRule {
            id: "r1".to_string(),
            name: "Chrome".to_string(),
            platform: Platform::Macos,
            process_name: vec!["Google Chrome".to_string()],
            process_path: Vec::new(),
            path_regex: Vec::new(),
            enabled: true,
        };
        let rules = state.store.upsert_rule(rule).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(state.store.rules().len(), 1);
        assert_eq!(state.store.delete_rule("r1").unwrap().len(), 0);
    }

    #[test]
    fn app_icon_with_missing_path_is_fallback() {
        // Контракт команды: отсутствующий бандл/файл — `None`, а не паника.
        let app = InstalledApp {
            name: "Ghost".to_string(),
            path: "/nonexistent/App.app".to_string(),
            platform: Platform::Macos,
        };
        assert!(app_icon(app).is_none());

        let unknown = InstalledApp {
            name: "Ghost".to_string(),
            path: "/nonexistent/ghost.exe".to_string(),
            platform: Platform::Windows,
        };
        assert!(app_icon(unknown).is_none());
    }

    #[test]
    fn logs_capture_events() {
        let state = state_with(Ok(fixture()));
        subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        let entries = state.logs.list(None, 10);
        assert!(!entries.is_empty());
    }

    #[test]
    fn connect_logs_proxy_dns_mode_without_warning() {
        let state = state_with(Ok(fixture()));
        subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        connect_inner(&state).unwrap();

        let infos = state.logs.list(Some(LogLevel::Info), 20);
        assert!(
            infos
                .iter()
                .any(|entry| entry.message.contains("режим proxy")),
            "при connect должен логироваться выбранный режим DNS"
        );
        assert!(
            state.logs.list(Some(LogLevel::Warn), 20).is_empty(),
            "в режиме proxy предупреждений об утечке DNS быть не должно"
        );
    }

    #[test]
    fn connect_logs_system_dns_leak_warning() {
        let state = state_with(Ok(fixture()));
        let mut settings = state.store.settings();
        settings.network.dns_mode = splitroute_protocol::DnsMode::System;
        settings_update_inner(&state, settings).unwrap();
        subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        connect_inner(&state).unwrap();

        let warnings = state.logs.list(Some(LogLevel::Warn), 10);
        assert!(
            warnings
                .iter()
                .any(|entry| entry.message.contains("DNS") && entry.message.contains("утечка")),
            "в режиме system нужен Warn об утечке DNS"
        );
    }

    #[test]
    fn settings_network_default_is_available() {
        let settings: Settings = AppSettings::default().network;
        assert_eq!(settings.mtu, 1500);
    }

    /// UI показывает причину установки, поэтому код должен различать случаи.
    #[test]
    fn installer_errors_map_to_distinct_codes() {
        let cases = [
            (InstallerError::RequiresPrivileges, "INSTALLER_PRIVILEGES"),
            (InstallerError::Unsupported, "INSTALLER_UNSUPPORTED"),
            (
                InstallerError::MissingBinary("splitroute-helper".to_string()),
                "INSTALLER_MISSING_BINARY",
            ),
            (
                InstallerError::Failed("операция отменена пользователем".to_string()),
                "INSTALLER_FAILED",
            ),
        ];
        for (error, code) in cases {
            // Текст установщика сохраняется, чтобы UI показал точную причину.
            let message = error.to_string();
            let command_error = CommandError::from(error);
            assert_eq!(command_error.code, code);
            assert_eq!(command_error.message, message);
        }
    }

    /// Первый вызов — ошибка авторизации, второй — успешная фикстура.
    struct FailThenOkFetcher {
        calls: std::sync::atomic::AtomicUsize,
        body: String,
    }

    impl SubscriptionFetcher for FailThenOkFetcher {
        fn fetch(&self, _request: &FetchRequest) -> Result<String, FetchError> {
            if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Err(FetchError::Unauthorized)
            } else {
                Ok(self.body.clone())
            }
        }
    }

    #[test]
    fn subscription_error_then_retry_succeeds() {
        let state = AppState::new(
            Arc::new(MockHelperClient::new()),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("retry")),
            Arc::new(FailThenOkFetcher {
                calls: std::sync::atomic::AtomicUsize::new(0),
                body: fixture(),
            }),
        );

        let error = subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, "UNAUTHORIZED");
        assert!(state.store.data().profiles.is_empty());

        let response = subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(response.profiles.len(), 2);
        assert_eq!(
            response.selected_profile_id.as_deref(),
            Some(response.profiles[0].id.as_str())
        );
    }

    #[test]
    fn end_to_end_subscription_location_rules_connect() {
        let path = temp_path("e2e");
        let helper = Arc::new(MockHelperClient::new());
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(&path),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        );

        // Пустое состояние: нет подписки, локации и правил.
        let empty = state.store.data();
        assert!(empty.profiles.is_empty());
        assert!(empty.selected_profile_id.is_none());
        assert!(empty.rules.is_empty());

        // Успешное обновление: два профиля, первый выбран автоматически.
        let fetched = subscription_fetch_inner(
            &state,
            Some("https://example.invalid/sub".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(fetched.profiles.len(), 2);
        let first = fetched.profiles[0].id.clone();
        let second = fetched.profiles[1].id.clone();
        assert_eq!(fetched.selected_profile_id.as_deref(), Some(first.as_str()));

        // Выбор второй локации сохраняется после «перезапуска».
        state.store.select_profile(&second).unwrap();
        drop(state);
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(&path),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        );
        assert_eq!(
            state.store.data().selected_profile_id.as_deref(),
            Some(second.as_str())
        );

        // allowlist: правило по имени процесса и правило по regex пути.
        let process_rule = AppRule {
            id: "rule-curl".to_string(),
            name: "curl".to_string(),
            platform: Platform::Macos,
            process_name: vec!["curl".to_string()],
            process_path: Vec::new(),
            path_regex: Vec::new(),
            enabled: true,
        };
        let regex_rule = AppRule {
            id: "rule-chrome".to_string(),
            name: "Google Chrome".to_string(),
            platform: Platform::Macos,
            process_name: Vec::new(),
            process_path: Vec::new(),
            path_regex: vec![r"^/Applications/Google Chrome\.app/".to_string()],
            enabled: true,
        };
        state.store.upsert_rule(process_rule).unwrap();
        state.store.upsert_rule(regex_rule.clone()).unwrap();
        assert_eq!(state.store.rules().len(), 2);
        state
            .store
            .upsert_rule(AppRule {
                enabled: false,
                ..regex_rule
            })
            .unwrap();

        // Подключение через mock helper: в конфиг попадает только включённое правило,
        // выбранная локация — вторая.
        let status = connect_inner(&state).unwrap();
        assert_eq!(status.state, splitroute_protocol::CoreState::Running);
        let config = helper.last_config().expect("конфиг передан helper'у");
        let route_rules = config["route"]["rules"].as_array().unwrap();
        assert!(route_rules
            .iter()
            .any(|rule| rule["process_name"][0] == "curl"));
        assert!(
            !route_rules
                .iter()
                .any(|rule| rule.get("process_path_regex").is_some()),
            "выключенное правило не должно попадать в конфиг"
        );
        assert_eq!(config["outbounds"][0]["server"], "203.0.113.20");

        // Отключение.
        assert_eq!(
            disconnect_inner(&state).unwrap().state,
            splitroute_protocol::CoreState::Stopped
        );
        let _ = std::fs::remove_file(&path);
    }
}
