//! Автообновление подписки по интервалу (M1, PRD §6.1).
//!
//! Расписание считается от последнего успешного ответа (`profiles_fetched_at`
//! в `state.json`) и последней попытки (в памяти процесса). Ошибка сети не
//! стирает кэш, но и не запускает новую попытку раньше интервала: иначе
//! недоступный провайдер получал бы запрос на каждой проверке расписания.
//!
//! Загрузка идёт тем же путём, что и ручное обновление
//! (`subscription_fetch_inner`), поэтому окно не ждёт сеть, а параллельные
//! загрузки исключены общим флагом в `AppState`.

use std::thread;
use std::time::Duration;

use splitroute_protocol::LogLevel;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{
    subscription_fetch_inner, AppState, SubscriptionFetchResponse, FETCH_IN_PROGRESS,
};
use crate::store::now_secs;

/// Событие UI: подписка обновлена в фоне. Payload — `SubscriptionFetchResponse`
/// (профили без `raw`, без URL подписки и HWID).
pub(crate) const UPDATED: &str = "subscription-updated";

/// Как часто планировщик проверяет расписание. Интервал задаётся в часах, так
/// что минуты достаточно: правка настроек применяется на ближайшей проверке.
const TICK_SECS: u64 = 60;

/// Интервал автообновления в секундах; `None` — автообновление выключено (`0`).
pub(crate) fn interval_secs(hours: u16) -> Option<u64> {
    (hours > 0).then(|| u64::from(hours) * 3600)
}

/// Момент, от которого отсчитывается следующий интервал: последний успех или
/// последняя попытка — что позже.
pub(crate) fn base_time(fetched_at: Option<u64>, last_attempt: Option<u64>) -> Option<u64> {
    match (fetched_at, last_attempt) {
        (Some(fetched), Some(attempt)) => Some(fetched.max(attempt)),
        (Some(fetched), None) => Some(fetched),
        (None, Some(attempt)) => Some(attempt),
        (None, None) => None,
    }
}

/// Когда ждать следующее автообновление. `None` — выключено или данных ещё нет
/// (тогда первая попытка выполняется сразу при запуске).
pub(crate) fn next_update_at(
    fetched_at: Option<u64>,
    last_attempt: Option<u64>,
    hours: u16,
) -> Option<u64> {
    let interval = interval_secs(hours)?;
    base_time(fetched_at, last_attempt).map(|base| base + interval)
}

/// Пора обновлять: интервал истёк или попыток ещё не было.
pub(crate) fn is_due(
    fetched_at: Option<u64>,
    last_attempt: Option<u64>,
    hours: u16,
    now: u64,
) -> bool {
    if interval_secs(hours).is_none() {
        return false;
    }
    next_update_at(fetched_at, last_attempt, hours).is_none_or(|next| now >= next)
}

/// Чем закончилась одна проверка расписания.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// Интервал `0`: автообновление выключено.
    Disabled,
    /// URL подписки не задан — обновлять нечего.
    NoUrl,
    /// Интервал ещё не истёк.
    NotDue,
    /// Загрузка уже идёт: параллельный запрос не нужен.
    Busy,
    /// Подписка обновлена.
    Updated(SubscriptionFetchResponse),
    /// Загрузка не удалась: кэш прежнего ответа сохранён, причина — в логе.
    Failed,
}

/// Одна проверка расписания. `now` передаётся явно: тесты не зависят от часов.
pub(crate) fn run(state: &AppState, now: u64) -> Outcome {
    let settings = state.store.settings();
    let hours = settings.subscription.auto_update_hours;
    if interval_secs(hours).is_none() {
        return Outcome::Disabled;
    }
    if settings.subscription.url.trim().is_empty() {
        return Outcome::NoUrl;
    }

    let fetched_at = state.store.data().profiles_fetched_at;
    if !is_due(fetched_at, state.last_fetch_attempt(), hours, now) {
        return Outcome::NotDue;
    }

    state.logs.push(LogLevel::Info, "Автообновление подписки");
    match subscription_fetch_inner(state, None, None, None) {
        Ok(response) => {
            state.set_last_fetch_attempt(now);
            state
                .logs
                .push(LogLevel::Info, "Автообновление подписки выполнено");
            Outcome::Updated(response)
        }
        Err(error) if error.code == FETCH_IN_PROGRESS => {
            state.logs.push(
                LogLevel::Debug,
                "Автообновление подписки: загрузка уже идёт — пропуск",
            );
            Outcome::Busy
        }
        Err(error) => {
            // Попытка запоминается и при ошибке: иначе сломанный провайдер
            // получал бы запрос на каждой проверке расписания.
            state.set_last_fetch_attempt(now);
            state.logs.push(
                LogLevel::Warn,
                format!("Автообновление подписки не удалось: {}", error.code),
            );
            Outcome::Failed
        }
    }
}

/// Запускает планировщик в фоне. Вызывается из `setup` после трея: ожидание
/// сети не должно задерживать появление окна.
pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || loop {
        if let Outcome::Updated(response) = run(&app.state::<AppState>(), now_secs()) {
            let _ = app.emit(UPDATED, response);
        }
        thread::sleep(Duration::from_secs(TICK_SECS));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privileged::{MockHelperClient, UnsupportedInstaller};
    use crate::store::{AppSettings, Store};
    use crate::subscription::{FetchError, FetchRequest, SubscriptionFetcher};
    use splitroute_protocol::{Profile, Protocol, TlsConfig, TlsKind, Transport};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    const URL: &str = "https://example.invalid/sub";
    const NOW: u64 = 1_800_000_000;

    /// Считает запросы: проверяем, что лишних загрузок не было.
    struct CountingFetcher {
        calls: AtomicUsize,
        body: Result<String, FetchError>,
    }

    impl CountingFetcher {
        fn succeeding() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                body: Ok(fixture()),
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                body: Err(FetchError::Network("нет сети".to_string())),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl SubscriptionFetcher for CountingFetcher {
        fn fetch(&self, _request: &FetchRequest) -> Result<String, FetchError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.body.clone()
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sr-autoupdate-{tag}-{}-{unique}.json",
            std::process::id()
        ))
    }

    fn fixture() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../spike/fixtures/subscription.json"
        ))
        .expect("фикстура подписки")
    }

    fn profile() -> Profile {
        Profile {
            id: "p1".to_string(),
            name: "Тестовая локация".to_string(),
            protocol: Protocol::Vless,
            server: "203.0.113.10".to_string(),
            port: 443,
            uuid: "d811a5ee-1d55-4dca-bd7f-f68b768a2241".to_string(),
            flow: None,
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

    fn state_with(fetcher: Arc<CountingFetcher>, tag: &str) -> AppState {
        let source: Arc<dyn SubscriptionFetcher> = fetcher;
        AppState::new(
            Arc::new(MockHelperClient::new()),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path(tag)),
            source,
        )
    }

    /// Сохраняет интервал и URL подписки, как это делает экран настроек.
    fn prepare(state: &AppState, hours: u16, url: &str) {
        let mut settings: AppSettings = state.store.settings();
        settings.subscription.auto_update_hours = hours;
        settings.subscription.url = url.to_string();
        state.store.update_settings(settings).unwrap();
    }

    #[test]
    fn zero_hours_disables_auto_update() {
        assert_eq!(interval_secs(0), None);
        assert!(!is_due(None, None, 0, NOW));
        assert_eq!(next_update_at(Some(1_000), None, 0), None);
    }

    #[test]
    fn interval_is_converted_from_hours() {
        assert_eq!(interval_secs(12), Some(43_200));
        assert_eq!(interval_secs(1), Some(3_600));
    }

    #[test]
    fn next_update_is_counted_from_last_success() {
        assert_eq!(next_update_at(Some(1_000), None, 1), Some(4_600));
        assert!(!is_due(Some(1_000), None, 1, 4_599));
        assert!(is_due(Some(1_000), None, 1, 4_600));
    }

    #[test]
    fn later_attempt_wins_over_older_success() {
        assert_eq!(base_time(Some(1_000), Some(2_000)), Some(2_000));
        assert_eq!(base_time(Some(2_000), Some(1_000)), Some(2_000));
        assert_eq!(base_time(None, Some(2_000)), Some(2_000));
        assert_eq!(base_time(None, None), None);
    }

    #[test]
    fn without_any_data_first_update_is_due() {
        assert_eq!(next_update_at(None, None, 12), None);
        assert!(is_due(None, None, 12, NOW));
    }

    #[test]
    fn disabled_interval_does_not_touch_network() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "disabled");
        prepare(&state, 0, URL);

        assert!(matches!(run(&state, NOW), Outcome::Disabled));
        assert_eq!(fetcher.calls(), 0);
        assert!(state.next_update_at().is_none());
    }

    #[test]
    fn empty_url_does_not_touch_network() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "no-url");
        prepare(&state, 6, "");

        assert!(matches!(run(&state, NOW), Outcome::NoUrl));
        assert_eq!(fetcher.calls(), 0);
        assert!(state.next_update_at().is_none(), "расписания нет");
    }

    #[test]
    fn fresh_subscription_is_not_fetched_again() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "fresh");
        prepare(&state, 12, URL);
        state.store.set_profiles(Vec::new(), NOW - 60).unwrap();

        assert!(matches!(run(&state, NOW), Outcome::NotDue));
        assert_eq!(fetcher.calls(), 0);
        assert_eq!(state.next_update_at(), Some(NOW - 60 + 43_200));
    }

    #[test]
    fn due_subscription_is_fetched_and_cache_updated() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "due");
        prepare(&state, 12, URL);

        let outcome = run(&state, NOW);
        assert!(matches!(outcome, Outcome::Updated(_)), "{outcome:?}");
        assert_eq!(fetcher.calls(), 1);
        assert_eq!(state.store.data().profiles.len(), 2);
        assert!(
            state.store.data().profiles_fetched_at.is_some(),
            "время успешного ответа сохранено"
        );
        // Отсчёт идёт от попытки (в тесте она синтетическая и позже системных
        // часов): следующее обновление — через интервал, а не сразу.
        assert_eq!(state.next_update_at(), Some(NOW + 43_200));
    }

    #[test]
    fn failure_keeps_cache_and_postpones_the_next_attempt() {
        let fetcher = CountingFetcher::failing();
        let state = state_with(fetcher.clone(), "failure");
        prepare(&state, 12, URL);
        // Кэш прошлого успешного ответа устарел, но сети нет.
        state.store.set_profiles(vec![profile()], 1).unwrap();

        let outcome = run(&state, NOW);
        assert!(matches!(outcome, Outcome::Failed), "{outcome:?}");
        assert_eq!(
            state.store.data().profiles.len(),
            1,
            "кэш последнего успешного ответа сохраняется"
        );
        assert_eq!(state.store.data().profiles_fetched_at, Some(1));
        let warnings = state.logs.list(Some(LogLevel::Warn), 20);
        assert!(warnings
            .iter()
            .any(|entry| entry.message.contains("Ошибка подписки: NETWORK")));
        assert!(warnings
            .iter()
            .all(|entry| !entry.message.contains("203.0.113")));

        // Следующая попытка — не раньше интервала, а не на каждом тике.
        assert!(matches!(run(&state, NOW + 60), Outcome::NotDue));
        assert_eq!(
            fetcher.calls(),
            1,
            "сломанный провайдер не опрашивается чаще"
        );
        assert_eq!(state.next_update_at(), Some(NOW + 43_200));
    }

    #[test]
    fn parallel_fetch_is_rejected() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "parallel");
        prepare(&state, 12, URL);

        let guard = state.try_begin_fetch().expect("первый захват");
        assert!(matches!(run(&state, NOW), Outcome::Busy));
        assert_eq!(
            subscription_fetch_inner(&state, None, None, None)
                .unwrap_err()
                .code,
            "FETCH_IN_PROGRESS"
        );
        assert_eq!(fetcher.calls(), 0);

        // После освобождения загрузка снова возможна.
        drop(guard);
        assert!(matches!(run(&state, NOW), Outcome::Updated(_)));
        assert_eq!(fetcher.calls(), 1);
    }
}
