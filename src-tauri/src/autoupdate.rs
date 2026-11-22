//! Автообновление подписок по интервалу (M1, PRD §6.1).
//!
//! Расписание у каждой подписки своё: от последнего успешного ответа
//! (`fetched_at` записи в `state.json`) и последней попытки (в памяти
//! процесса). Ошибка сети не стирает кэш, но и не запускает новую попытку
//! раньше интервала: иначе недоступный провайдер получал бы запрос на каждой
//! проверке расписания.
//!
//! Загрузка идёт тем же путём, что и ручное обновление
//! (`subscription_fetch_inner`), поэтому окно не ждёт сеть, а параллельные
//! загрузки одной подписки исключены guard'ом в `AppState`. Разные подписки
//! друг другу не мешают.

use std::thread;
use std::time::Duration;

use splitroute_protocol::LogLevel;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{
    subscription_fetch_inner, AppState, SubscriptionFetchResponse, FETCH_IN_PROGRESS,
};
use crate::store::{now_secs, SubscriptionState};

/// Событие UI: одна из подписок обновлена в фоне. Payload —
/// `SubscriptionFetchResponse` (профили без `raw`, без URL подписки и HWID).
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

/// Чем закончилась проверка расписания одной подписки.
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

/// Результат проверки расписания одной подписки: `(идентификатор, итог)`.
pub(crate) type SubOutcome = (String, Outcome);

/// Одна проверка расписания подписки. `now` передаётся явно: тесты не зависят
/// от часов.
pub(crate) fn run_subscription(
    state: &AppState,
    subscription: &SubscriptionState,
    now: u64,
) -> Outcome {
    let hours = subscription.auto_update_hours;
    if interval_secs(hours).is_none() {
        return Outcome::Disabled;
    }
    if subscription.url.trim().is_empty() {
        return Outcome::NoUrl;
    }

    if !is_due(
        subscription.fetched_at,
        state.last_fetch_attempt(&subscription.id),
        hours,
        now,
    ) {
        return Outcome::NotDue;
    }

    state.logs.push(LogLevel::Info, "Автообновление подписки");
    match subscription_fetch_inner(state, &subscription.id, now) {
        Ok(response) => {
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
            state.logs.push(
                LogLevel::Warn,
                format!("Автообновление подписки не удалось: {}", error.code),
            );
            Outcome::Failed
        }
    }
}

/// Проверяет расписание всех подписок. Подписки обновляются по очереди:
/// запросы к разным провайдерам не перемешиваются, а окно всё равно не ждёт.
pub(crate) fn run(state: &AppState, now: u64) -> Vec<SubOutcome> {
    state
        .store
        .subscriptions()
        .iter()
        .map(|subscription| {
            (
                subscription.id.clone(),
                run_subscription(state, subscription, now),
            )
        })
        .collect()
}

/// Запускает планировщик в фоне. Вызывается из `setup` после трея: ожидание
/// сети не должно задерживать появление окна.
pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || loop {
        for (_id, outcome) in run(&app.state::<AppState>(), now_secs()) {
            if let Outcome::Updated(response) = outcome {
                let _ = app.emit(UPDATED, response);
            }
        }
        thread::sleep(Duration::from_secs(TICK_SECS));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandError;
    use crate::privileged::{MockHelperClient, UnsupportedInstaller};
    use crate::store::Store;
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

    fn profile(id: &str) -> Profile {
        Profile {
            id: id.to_string(),
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

    /// Создаёт подписку с интервалом и URL, как это делает экран настроек.
    fn prepare(state: &AppState, hours: u16, url: &str) -> String {
        let entry = state.store.add_subscription("", url, "", hours).unwrap();
        entry.id
    }

    /// Итог проверки конкретной подписки.
    fn outcome_of<'a>(outcomes: &'a [SubOutcome], id: &str) -> &'a Outcome {
        &outcomes
            .iter()
            .find(|(subscription_id, _)| subscription_id == id)
            .expect("итог по подписке")
            .1
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
        let id = prepare(&state, 0, URL);

        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, &id), Outcome::Disabled));
        assert_eq!(fetcher.calls(), 0);
        let entry = state.store.subscription(&id).unwrap();
        assert!(state.next_update_at(&entry).is_none());
    }

    /// Подписка с пустым URL (возможна только правкой state.json — add/update
    /// пустой URL отвергают) не попадает в сеть: защита на случай ручной правки.
    #[test]
    fn empty_url_does_not_touch_network() {
        let fetcher = CountingFetcher::succeeding();
        let path = temp_path("no-url");
        let mut data = crate::store::StoredData::default();
        data.settings.subscription.hwid = "0123456789abcdef".to_string();
        data.subscriptions.push(SubscriptionState {
            id: "s1".to_string(),
            name: String::new(),
            url: "  ".to_string(),
            user_agent: String::new(),
            auto_update_hours: 6,
            profiles: Vec::new(),
            disabled_profiles: Vec::new(),
            fetched_at: None,
        });
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let state = AppState::new(
            Arc::new(MockHelperClient::new()),
            Arc::new(UnsupportedInstaller),
            Store::load(&path),
            fetcher.clone(),
        );

        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, "s1"), Outcome::NoUrl));
        assert_eq!(fetcher.calls(), 0);
        let entry = state.store.subscription("s1").unwrap();
        assert!(state.next_update_at(&entry).is_none(), "расписания нет");
    }

    #[test]
    fn fresh_subscription_is_not_fetched_again() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "fresh");
        let id = prepare(&state, 12, URL);
        state
            .store
            .set_subscription_profiles(&id, Vec::new(), NOW - 60)
            .unwrap();

        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, &id), Outcome::NotDue));
        assert_eq!(fetcher.calls(), 0);
        let entry = state.store.subscription(&id).unwrap();
        assert_eq!(state.next_update_at(&entry), Some(NOW - 60 + 43_200));
    }

    #[test]
    fn due_subscription_is_fetched_and_cache_updated() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "due");
        let id = prepare(&state, 12, URL);

        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, &id), Outcome::Updated(_)));
        assert_eq!(fetcher.calls(), 1);
        let entry = state.store.subscription(&id).unwrap();
        assert_eq!(entry.profiles.len(), 2);
        assert!(
            entry.fetched_at.is_some(),
            "время успешного ответа сохранено"
        );
        // Отсчёт идёт от попытки: следующее обновление — через интервал,
        // а не на каждом тике.
        assert_eq!(state.next_update_at(&entry), Some(NOW + 43_200));
    }

    #[test]
    fn failure_keeps_cache_and_postpones_the_next_attempt() {
        let fetcher = CountingFetcher::failing();
        let state = state_with(fetcher.clone(), "failure");
        let id = prepare(&state, 12, URL);
        // Кэш прошлого успешного ответа устарел, но сети нет.
        state
            .store
            .set_subscription_profiles(&id, vec![profile("p1")], 1)
            .unwrap();

        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, &id), Outcome::Failed));
        let entry = state.store.subscription(&id).unwrap();
        assert_eq!(
            entry.profiles.len(),
            1,
            "кэш последнего успешного ответа сохраняется"
        );
        assert_eq!(entry.fetched_at, Some(1));
        let warnings = state.logs.list(Some(LogLevel::Warn), 20);
        assert!(warnings
            .iter()
            .any(|entry| entry.message.contains("Ошибка подписки: NETWORK")));
        assert!(warnings
            .iter()
            .all(|entry| !entry.message.contains("203.0.113")));

        // Следующая попытка — не раньше интервала, а не на каждом тике.
        assert!(matches!(
            outcome_of(&run(&state, NOW + 60), &id),
            Outcome::NotDue
        ));
        assert_eq!(
            fetcher.calls(),
            1,
            "сломанный провайдер не опрашивается чаще"
        );
        assert_eq!(state.next_update_at(&entry), Some(NOW + 43_200));
    }

    /// Проверяет расписание нескольких подписок: обновляется только истёкшая.
    #[test]
    fn scheduler_updates_each_due_subscription_independently() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "multi");
        let due = prepare(&state, 1, URL);
        let fresh = prepare(&state, 12, URL);
        let disabled = prepare(&state, 0, URL);
        state
            .store
            .set_subscription_profiles(&fresh, vec![profile("p1")], NOW - 60)
            .unwrap();

        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, &due), Outcome::Updated(_)));
        assert!(matches!(outcome_of(&outcomes, &fresh), Outcome::NotDue));
        assert!(matches!(
            outcome_of(&outcomes, &disabled),
            Outcome::Disabled
        ));
        assert_eq!(
            fetcher.calls(),
            1,
            "загрузка одна: только истёкший интервал"
        );
        // Все три подписки прошли проверку расписания.
        assert_eq!(outcomes.len(), 3);
    }

    /// Ручная попытка (в том числе неудачная) переносит расписание этой
    /// подписки на полный интервал (PRD §6.1).
    #[test]
    fn manual_attempt_postpones_the_schedule() {
        let fetcher = CountingFetcher::failing();
        let state = state_with(fetcher.clone(), "manual");
        let id = prepare(&state, 12, URL);

        let error = subscription_fetch_inner(&state, &id, NOW).unwrap_err();
        assert_eq!(error.code, "NETWORK");
        assert_eq!(fetcher.calls(), 1);
        let entry = state.store.subscription(&id).unwrap();
        assert_eq!(entry.fetched_at, None, "кэш пуст: ответа не было");
        assert_eq!(
            state.next_update_at(&entry),
            Some(NOW + 43_200),
            "неудачная ручная попытка тоже отодвигает автообновление"
        );
        assert!(matches!(
            outcome_of(&run(&state, NOW + 60), &id),
            Outcome::NotDue
        ));
        assert_eq!(fetcher.calls(), 1);
    }

    #[test]
    fn parallel_fetch_of_one_subscription_is_rejected() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "parallel");
        let id = prepare(&state, 12, URL);

        let guard = state.try_begin_fetch(&id).expect("первый захват");
        assert!(matches!(outcome_of(&run(&state, NOW), &id), Outcome::Busy));
        assert_eq!(
            subscription_fetch_inner(&state, &id, NOW).unwrap_err().code,
            "FETCH_IN_PROGRESS"
        );
        assert_eq!(fetcher.calls(), 0);

        // После освобождения загрузка снова возможна.
        drop(guard);
        assert!(matches!(
            outcome_of(&run(&state, NOW), &id),
            Outcome::Updated(_)
        ));
        assert_eq!(fetcher.calls(), 1);
    }

    /// Разные подписки не блокируют друг друга: guard на одну подписку не
    /// мешает загрузке другой.
    #[test]
    fn parallel_fetch_of_different_subscriptions_is_allowed() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "parallel-multi");
        let first = prepare(&state, 12, URL);
        let second = prepare(&state, 12, URL);

        let _guard = state.try_begin_fetch(&first).expect("захват первой");
        let outcomes = run(&state, NOW);
        assert!(matches!(outcome_of(&outcomes, &first), Outcome::Busy));
        assert!(matches!(
            outcome_of(&outcomes, &second),
            Outcome::Updated(_)
        ));
        assert_eq!(fetcher.calls(), 1);
    }

    /// Неизвестный идентификатор подписки даёт типизированную ошибку.
    #[test]
    fn fetch_unknown_subscription_is_not_found() {
        let fetcher = CountingFetcher::succeeding();
        let state = state_with(fetcher.clone(), "unknown");
        let error: CommandError = subscription_fetch_inner(&state, "missing", NOW).unwrap_err();
        assert_eq!(error.code, "SUBSCRIPTION_NOT_FOUND");
        assert_eq!(fetcher.calls(), 0);
    }
}
