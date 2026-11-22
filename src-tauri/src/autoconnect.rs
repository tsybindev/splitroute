//! Автоподключение при запуске приложения (B2, PRD §6.5).
//!
//! Настройка `preferences.connect_on_start` сохраняется в `state.json`; здесь
//! она применяется один раз при старте. Решение и запуск ядра выполняются в
//! фоновом потоке: окно не должно ждать helper'а, а неудача — это штатное
//! состояние «ошибка», а не паника.

use splitroute_protocol::{CoreState, HelperStatus, LogLevel};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{connect_inner, AppState, CommandError};
use crate::tray;

/// Событие UI: автоподключение не удалось. Payload — `CommandError`
/// (`code` + русский текст), без URL подписки, HWID и адресов серверов.
pub(crate) const AUTOCONNECT_FAILED: &str = "autoconnect-failed";

/// Чем закончился один проход автоподключения.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// Настройка выключена.
    Disabled,
    /// Сохранённой локации нет: подключаться некуда.
    NoProfile,
    /// Ядро уже запущено — повторный `start` не нужен.
    AlreadyRunning(HelperStatus),
    /// Подключение выполнено.
    Connected(HelperStatus),
    /// Подключение не удалось: причина сохранена для UI.
    Failed(CommandError),
}

impl Outcome {
    /// Состояние ядра, которое нужно отразить в трее и UI.
    pub(crate) fn status(&self) -> Option<&HelperStatus> {
        match self {
            Self::AlreadyRunning(status) | Self::Connected(status) => Some(status),
            _ => None,
        }
    }

    /// Ошибка автоподключения, если она была.
    pub(crate) fn error(&self) -> Option<&CommandError> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }
}

/// Один проход автоподключения.
///
/// Выполняется только при включённой настройке и сохранённой локации. Если ядро
/// уже запущено, `start` не повторяется: helper вернул бы `ALREADY_RUNNING`.
pub(crate) fn run(state: &AppState) -> Outcome {
    if !state.store.settings().preferences.connect_on_start {
        return Outcome::Disabled;
    }

    let data = state.store.data();
    let has_profile = data
        .selected_profile_id
        .as_deref()
        .is_some_and(|id| data.find_profile(id).is_some());
    if !has_profile {
        state.logs.push(
            LogLevel::Debug,
            "Автоподключение: локация не выбрана — пропуск",
        );
        return Outcome::NoProfile;
    }
    drop(data);

    // Ошибку недоступного helper'а отдаст `connect_inner`: дублировать её здесь
    // не нужно, а лишний `start` без ядра безвреден.
    if let Ok(status) = state.helper.status() {
        if status.state == CoreState::Running {
            state.logs.push(
                LogLevel::Debug,
                "Автоподключение: ядро уже запущено — пропуск",
            );
            return Outcome::AlreadyRunning(status);
        }
    }

    match connect_inner(state) {
        Ok(status) => Outcome::Connected(status),
        Err(error) => {
            state.logs.push(
                LogLevel::Warn,
                format!("Автоподключение не удалось: {}", error.code),
            );
            state.set_startup_error(Some(error.clone()));
            Outcome::Failed(error)
        }
    }
}

/// Запускает автоподключение в фоне, если настройка включена.
///
/// Вызывается из `setup` после создания трея: блокирующий IPC с helper'ом не
/// должен задерживать появление окна.
pub(crate) fn spawn(app: AppHandle) {
    if !app
        .state::<AppState>()
        .store
        .settings()
        .preferences
        .connect_on_start
    {
        return;
    }

    tauri::async_runtime::spawn_blocking(move || {
        let outcome = run(&app.state::<AppState>());
        if let Some(status) = outcome.status().cloned() {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || tray::publish(&handle, &status));
        }
        if let Some(error) = outcome.error() {
            let _ = app.emit(AUTOCONNECT_FAILED, error);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privileged::{
        HelperClient, HelperClientError, MockHelperClient, UnsupportedInstaller,
    };
    use crate::store::{AppSettings, Store};
    use crate::subscription::{FetchError, FetchRequest, SubscriptionFetcher};
    use splitroute_protocol::{
        ErrorCode, Platform, Profile, Protocol, TlsConfig, TlsKind, Transport,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Автоподключение не загружает подписку: сеть в тестах не нужна.
    struct NoFetcher;

    impl SubscriptionFetcher for NoFetcher {
        fn fetch(&self, _request: &FetchRequest) -> Result<String, FetchError> {
            Err(FetchError::MissingUrl)
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sr-autoconnect-{tag}-{}-{unique}.json",
            std::process::id()
        ))
    }

    fn profile() -> Profile {
        Profile {
            id: "p1".to_string(),
            name: "Тестовая локация".to_string(),
            protocol: Protocol::Vless,
            server: "203.0.113.10".to_string(),
            port: 443,
            uuid: "d811a5ee-1d55-4dca-bd7f-f68b768a2241".to_string(),
            flow: Some("xtls-rprx-vision".to_string()),
            tls: TlsConfig {
                kind: TlsKind::Reality,
                server_name: "example.com".to_string(),
                public_key: Some("public-key".to_string()),
                short_id: None,
                fingerprint: None,
                alpn: Vec::new(),
            },
            transport: Transport::Tcp,
            raw: None,
        }
    }

    fn state_with(helper: Arc<MockHelperClient>, tag: &str) -> AppState {
        AppState::new(
            helper,
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path(tag)),
            Arc::new(NoFetcher),
        )
    }

    /// Включает настройку и (если нужно) кладёт профиль в состояние.
    fn prepare(state: &AppState, with_profile: bool) {
        let mut settings: AppSettings = state.store.settings();
        settings.preferences.connect_on_start = true;
        state.store.update_settings(settings).unwrap();
        if with_profile {
            seed_profiles(state, &[profile()]);
        }
    }

    /// Создаёт подписку и кладёт в её кэш заданные профили.
    fn seed_profiles(state: &AppState, profiles: &[Profile]) -> String {
        let entry = state
            .store
            .add_subscription("", "https://example.invalid/sub", "", 12)
            .unwrap();
        state
            .store
            .set_subscription_profiles(&entry.id, profiles.to_vec(), 1)
            .unwrap();
        entry.id
    }

    #[test]
    fn disabled_setting_does_not_start_core() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone(), "disabled");
        seed_profiles(&state, &[profile()]);

        assert!(matches!(run(&state), Outcome::Disabled));
        assert_eq!(helper.start_calls(), 0);
        assert!(state.startup_error().is_none());
    }

    #[test]
    fn enabled_without_profile_is_skipped() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone(), "no-profile");
        prepare(&state, false);

        assert!(matches!(run(&state), Outcome::NoProfile));
        assert_eq!(helper.start_calls(), 0, "без локации ядро не запускается");
        assert!(state.startup_error().is_none(), "это не ошибка для UI");
    }

    #[test]
    fn enabled_with_profile_connects_once() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone(), "connect");
        prepare(&state, true);

        let outcome = run(&state);
        let status = outcome.status().expect("статус для трея");
        assert_eq!(status.state, CoreState::Running);
        assert!(outcome.error().is_none());
        assert_eq!(helper.start_calls(), 1);
        assert_eq!(
            helper.last_config().unwrap()["outbounds"][0]["server"],
            "203.0.113.10"
        );
        assert!(state.startup_error().is_none());
    }

    #[test]
    fn already_running_core_is_not_started_again() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone(), "running");
        prepare(&state, true);
        // Ядро поднято прошлой сессией или другим путём.
        helper
            .start(serde_json::json!({}), serde_json::json!({}), false)
            .unwrap();

        let outcome = run(&state);
        assert!(matches!(outcome, Outcome::AlreadyRunning(_)));
        assert_eq!(helper.start_calls(), 1, "повторного start быть не должно");
        assert!(state.startup_error().is_none());
    }

    /// Первый `start` падает, последующие проходят: приложение остаётся рабочим.
    struct FailFirstStartHelper {
        inner: MockHelperClient,
        starts: AtomicU32,
    }

    impl HelperClient for FailFirstStartHelper {
        fn status(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.status()
        }

        fn start(
            &self,
            config: serde_json::Value,
            rule_set: serde_json::Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            if self.starts.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(HelperClientError::Helper {
                    code: ErrorCode::StartFailed,
                    message: "синтетическая ошибка старта".to_string(),
                });
            }
            self.inner.start(config, rule_set, kill_switch)
        }

        fn reload_routing(
            &self,
            rule_set: serde_json::Value,
        ) -> Result<HelperStatus, HelperClientError> {
            self.inner.reload_routing(rule_set)
        }

        fn restart(
            &self,
            config: serde_json::Value,
            rule_set: serde_json::Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            self.inner.restart(config, rule_set, kill_switch)
        }

        fn stop(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.stop()
        }
    }

    #[test]
    fn start_failure_is_reported_and_app_stays_usable() {
        let helper = Arc::new(FailFirstStartHelper {
            inner: MockHelperClient::new(),
            starts: AtomicU32::new(0),
        });
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("failure")),
            Arc::new(NoFetcher),
        );
        prepare(&state, true);

        let outcome = run(&state);
        let error = outcome.error().expect("ошибка автоподключения");
        assert_eq!(error.code, "HELPER_ERROR");
        assert!(outcome.status().is_none());
        assert_eq!(
            state
                .startup_error()
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("HELPER_ERROR"),
            "ошибка доступна UI и не теряется"
        );

        let warnings = state.logs.list(Some(LogLevel::Warn), 20);
        assert!(warnings
            .iter()
            .any(|entry| entry.message.contains("Автоподключение не удалось")));
        assert!(
            warnings
                .iter()
                .all(|entry| !entry.message.contains("203.0.113")),
            "адрес сервера в лог не попадает"
        );

        // Приложение остаётся работоспособным: ручное подключение проходит.
        let status = connect_inner(&state).unwrap();
        assert_eq!(status.state, CoreState::Running);
    }

    #[test]
    fn stale_selection_is_treated_as_missing_profile() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone(), "stale");
        prepare(&state, true);
        // Профиль выбран, но подписка обновилась и его больше нет.
        let id = state.store.subscriptions()[0].id.clone();
        state
            .store
            .set_subscription_profiles(&id, Vec::new(), 2)
            .unwrap();
        assert!(state.store.data().selected_profile_id.is_none());
        state
            .store
            .upsert_rule(splitroute_protocol::AppRule {
                id: "r1".to_string(),
                name: "curl".to_string(),
                platform: Platform::Macos,
                process_name: vec!["curl".to_string()],
                process_path: Vec::new(),
                path_regex: Vec::new(),
                enabled: true,
            })
            .unwrap();

        assert!(matches!(run(&state), Outcome::NoProfile));
        assert_eq!(helper.start_calls(), 0);
    }
}
