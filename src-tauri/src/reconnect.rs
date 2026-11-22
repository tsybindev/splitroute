//! Автореконнект ядра после падения (S4-T15, PRD §13 п. 4).
//!
//! Детект живёт в Rust, а не в UI: фоновый надзор работает и при закрытом
//! окне (tray-only), и без kill switch. Серия выполняется через общий
//! `connect_inner` — без дублирования логики кнопки. Правила kill switch
//! между попытками и при исчерпании серии не трогаются (fail-closed,
//! PRD §6.2): их снимает только ручное отключение.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::MutexGuard;
use std::time::Duration;

use serde::Serialize;
use splitroute_protocol::{CoreState, HelperStatus, LogLevel};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{connect_inner, AppState, CommandError};
use crate::tray;

/// Сколько попыток переподключения выполняет одна серия (PRD §13 п. 4).
pub(crate) const MAX_ATTEMPTS: u32 = 3;

/// Пауза между попытками серии, мс. Атомик — только чтобы юнит-тесты не ждали
/// реальные секунды; в рантайме значение не меняется.
pub(crate) static RETRY_DELAY_MS: AtomicU64 = AtomicU64::new(3000);

fn retry_delay() -> Duration {
    Duration::from_millis(RETRY_DELAY_MS.load(Ordering::Relaxed))
}

/// Как часто надзор опрашивает helper при активной сессии. Как и фронтовый
/// опрос kill switch — 2 с: UI показывает `CORE_STOPPED` без новой задержки.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);

/// Шаг сна внутри паузы: ручное отключение прерывает серию быстрее паузы.
const CANCEL_CHECK: Duration = Duration::from_millis(200);

/// Событие UI: серия автореконнекта пошла, началась попытка N из M.
pub(crate) const RECONNECT_PROGRESS: &str = "reconnect-progress";

/// Событие UI: серия исчерпана — требуется ручное действие пользователя.
pub(crate) const RECONNECT_FAILED: &str = "reconnect-failed";

/// Состояние серии автореконнекта. Живёт в `AppState` под мьютексом.
#[derive(Default)]
pub(crate) struct ReconnectState {
    /// Сессия активна: успешный `start` был, ручного отключения не было.
    watching: bool,
    /// Номер выполняющейся попытки серии (0 — серии нет).
    attempts: u32,
    /// Серия исчерпана: причина ждёт ручного действия пользователя.
    exhausted: Option<CommandError>,
    /// Серия выполняется в фоне: параллельная серия не запускается.
    running: bool,
}

/// Состояние серии для UI: текущая попытка и, если серия исчерпана, причина.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconnectStatus {
    pub attempt: u32,
    pub max_attempts: u32,
    pub exhausted: Option<CommandError>,
}

/// Чем закончилась одна серия переподключений.
#[derive(Debug)]
pub(crate) enum SeriesOutcome {
    /// Ядро снова работает (подключение восстановлено этой серией или вручную).
    Restored(HelperStatus),
    /// Все попытки не удались: причина сохранена для UI, правила не тронуты.
    Exhausted(CommandError),
    /// Серия отменена: ручное отключение или параллельная серия.
    Cancelled,
}

impl AppState {
    fn lock_reconnect(&self) -> MutexGuard<'_, ReconnectState> {
        self.reconnect
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Успешный старт открывает наблюдение и сбрасывает серию: счётчик попыток
    /// сбрасывается успешным стартом (PRD §13 п. 4).
    pub(crate) fn reconnect_arm(&self) {
        let mut guard = self.lock_reconnect();
        guard.watching = true;
        guard.attempts = 0;
        guard.exhausted = None;
    }

    /// Ручное отключение закрывает наблюдение: падений больше не ожидается.
    pub(crate) fn reconnect_disarm(&self) {
        let mut guard = self.lock_reconnect();
        guard.watching = false;
        guard.attempts = 0;
        guard.exhausted = None;
    }

    /// Сессия активна: надзор должен следить за ядром.
    pub(crate) fn reconnect_watching(&self) -> bool {
        self.lock_reconnect().watching
    }

    /// Захватывает право на серию: параллельные тики не создают дубль, а
    /// исчерпанная серия не повторяется до ручного действия. Guard снимает
    /// признак при выходе — ошибка или паника не блокируют следующие серии.
    fn reconnect_begin_series(&self) -> Option<SeriesGuard<'_>> {
        let mut guard = self.lock_reconnect();
        if guard.running || guard.exhausted.is_some() {
            return None;
        }
        guard.running = true;
        guard.attempts = 0;
        guard.exhausted = None;
        Some(SeriesGuard(self))
    }

    fn reconnect_end_series(&self) {
        self.lock_reconnect().running = false;
    }

    fn reconnect_set_attempt(&self, attempt: u32) {
        self.lock_reconnect().attempts = attempt;
    }

    fn reconnect_exhaust(&self, error: CommandError) {
        self.lock_reconnect().exhausted = Some(error);
    }

    /// Состояние серии для команды `reconnect_status` и payload событий.
    pub(crate) fn reconnect_snapshot(&self) -> ReconnectStatus {
        let guard = self.lock_reconnect();
        ReconnectStatus {
            attempt: guard.attempts,
            max_attempts: MAX_ATTEMPTS,
            exhausted: guard.exhausted.clone(),
        }
    }
}

/// Право на серию: `running` снимается при любом выходе из `run_series`.
struct SeriesGuard<'a>(&'a AppState);

impl Drop for SeriesGuard<'_> {
    fn drop(&mut self) {
        self.0.reconnect_end_series();
    }
}

/// Запускает фоновый надзор за ядром. Вызывается один раз в `setup`.
///
/// Пока сессии нет, цикл не обращается к helper'у: один IPC-запрос статуса
/// раз в `WATCH_INTERVAL` только при активном подключении.
pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || watch_loop(app));
}

fn watch_loop(app: AppHandle) {
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let state = app.state::<AppState>();
        if !state.reconnect_watching() {
            continue;
        }
        match state.helper.status() {
            Ok(status) if status.state == CoreState::Stopped => execute_series(&app, &state),
            // Ошибка статуса — не падение ядра: переподключаемся только по
            // явному `state: stopped` от живого helper'а.
            Ok(_) => {}
            Err(error) => state.logs.push(
                LogLevel::Debug,
                format!("Автореконнект: статус helper'а недоступен ({error})"),
            ),
        }
    }
}

fn execute_series(app: &AppHandle, state: &AppState) {
    let emit_app = app.clone();
    let outcome = run_series(state, |snapshot| {
        let _ = emit_app.emit(RECONNECT_PROGRESS, snapshot);
    });
    match outcome {
        SeriesOutcome::Restored(status) => tray::publish(app, &status),
        SeriesOutcome::Exhausted(error) => {
            let _ = app.emit(RECONNECT_FAILED, error);
        }
        SeriesOutcome::Cancelled => {}
    }
}

/// Одна серия переподключений: до `MAX_ATTEMPTS` попыток через общий
/// `connect_inner` с паузой между ними. Серия отменяется ручным отключением
/// (`disconnect` ставит `watching = false`) и если ядро уже поднялось без неё.
///
/// `on_attempt` вызывается перед каждой попыткой — так UI получает прогресс
/// без ожидания конца серии.
pub(crate) fn run_series(state: &AppState, on_attempt: impl Fn(&ReconnectStatus)) -> SeriesOutcome {
    let Some(_guard) = state.reconnect_begin_series() else {
        return SeriesOutcome::Cancelled;
    };
    for attempt in 1..=MAX_ATTEMPTS {
        if !state.reconnect_watching() {
            return SeriesOutcome::Cancelled;
        }
        // Ядро могло подняться вручную во время паузы: серия больше не нужна.
        if let Ok(status) = state.helper.status() {
            if status.state == CoreState::Running {
                return SeriesOutcome::Restored(status);
            }
        }

        state.reconnect_set_attempt(attempt);
        on_attempt(&state.reconnect_snapshot());
        state.logs.push(
            LogLevel::Info,
            format!("Автореконнект: попытка {attempt} из {MAX_ATTEMPTS}"),
        );
        match connect_inner(state) {
            Ok(status) => {
                // `connect_inner` уже сбросил серию успешным стартом.
                state.logs.push(
                    LogLevel::Info,
                    format!("Автореконнект: подключение восстановлено (попытка {attempt})"),
                );
                return SeriesOutcome::Restored(status);
            }
            Err(error) => {
                // Код без текста: в логах нет деталей ошибок helper'а и
                // никакого содержимого конфига.
                state.logs.push(
                    LogLevel::Warn,
                    format!(
                        "Автореконнект: попытка {attempt} не удалась ({})",
                        error.code
                    ),
                );
            }
        }
        if attempt < MAX_ATTEMPTS && !sleep_cancellable(state, retry_delay()) {
            return SeriesOutcome::Cancelled;
        }
    }
    let error = CommandError::new(
        "RECONNECT_FAILED",
        "ядро не удалось запустить повторно автоматически",
    );
    state.reconnect_exhaust(error.clone());
    state.logs.push(
        LogLevel::Warn,
        format!(
            "Автореконнект: {MAX_ATTEMPTS} попыток не удались — требуется ручное \
             подключение; правила kill switch сохраняются"
        ),
    );
    SeriesOutcome::Exhausted(error)
}

/// Спит паузу между попытками, проверяя отмену каждые `CANCEL_CHECK`:
/// ручное отключение не должно ждать конца паузы. `false` — серия отменена.
fn sleep_cancellable(state: &AppState, total: Duration) -> bool {
    let mut waited = Duration::ZERO;
    while waited < total {
        std::thread::sleep(CANCEL_CHECK);
        waited += CANCEL_CHECK;
        if !state.reconnect_watching() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privileged::{
        HelperClient, HelperClientError, MockHelperClient, UnsupportedInstaller,
    };
    use crate::store::Store;
    use crate::subscription::{FetchError, FetchRequest, SubscriptionFetcher};
    use splitroute_protocol::{ErrorCode, Profile, Protocol, TlsConfig, TlsKind, Transport};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Тесты не должны ждать реальную паузу между попытками.
    fn fast_delay() {
        RETRY_DELAY_MS.store(1, Ordering::Relaxed);
    }

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
        // Счётчик против коллизий: тесты в одном процессе стартуют одновременно,
        // наносекундные метки могут совпасть.
        static SEQ: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "sr-reconnect-{tag}-{}-{}-{unique}.json",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed),
        ))
    }

    fn state_with(helper: Arc<dyn HelperClient>) -> AppState {
        AppState::new(
            helper,
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("state")),
            Arc::new(NoFetcher),
        )
    }

    /// Тестовый профиль, как в подписке: `connect_inner` собирает из него конфиг.
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

    /// Состояние «подключённая сессия, ядро упало»: профиль выбран, серия
    /// открыта (`watching = true`, счётчики сброшены) — как после падения ядра.
    fn arm_connected_session(state: &AppState) {
        let entry = state
            .store
            .add_subscription("", "https://example.invalid/sub", "", 12)
            .unwrap();
        state
            .store
            .set_subscription_profiles(&entry.id, vec![profile()], 1)
            .unwrap();
        state.store.select_profile(&profile().id).unwrap();
        state.reconnect_arm();
    }

    fn no_progress(_: &ReconnectStatus) {}

    /// Первый `start` падает, последующие проходят: серия со второй попытки.
    struct FailFirstStartHelper {
        inner: MockHelperClient,
        starts: AtomicU32,
    }

    impl FailFirstStartHelper {
        fn new() -> Self {
            Self {
                inner: MockHelperClient::new(),
                starts: AtomicU32::new(0),
            }
        }
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

        fn stop(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.stop()
        }
    }

    /// Все попытки старта падают: серия должна исчерпаться.
    struct AlwaysFailingHelper {
        inner: MockHelperClient,
        starts: AtomicU32,
    }

    impl AlwaysFailingHelper {
        fn new() -> Self {
            Self {
                inner: MockHelperClient::new(),
                starts: AtomicU32::new(0),
            }
        }
    }

    impl HelperClient for AlwaysFailingHelper {
        fn status(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.status()
        }

        fn start(
            &self,
            _config: serde_json::Value,
            _rule_set: serde_json::Value,
            _kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Err(HelperClientError::Helper {
                code: ErrorCode::StartFailed,
                message: "синтетическая ошибка старта".to_string(),
            })
        }

        fn reload_routing(
            &self,
            rule_set: serde_json::Value,
        ) -> Result<HelperStatus, HelperClientError> {
            self.inner.reload_routing(rule_set)
        }

        fn stop(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.stop()
        }
    }

    /// Серия не нужна: ядро поднялось до серии — попытки не тратятся.
    #[test]
    fn already_running_core_is_not_restarted() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);
        helper
            .start(serde_json::json!({}), serde_json::json!({}), false)
            .unwrap();

        let outcome = run_series(&state, no_progress);
        assert!(matches!(outcome, SeriesOutcome::Restored(_)));
        assert_eq!(
            helper.start_calls(),
            1,
            "ручной start — единственный: серия не стартует повторно"
        );
    }

    #[test]
    fn series_retries_and_restores_on_second_attempt() {
        fast_delay();
        let helper = Arc::new(FailFirstStartHelper::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);

        let attempts: Mutex<Vec<u32>> = Mutex::new(Vec::new());
        let outcome = run_series(&state, |snapshot| {
            attempts.lock().unwrap().push(snapshot.attempt);
        });
        assert!(matches!(outcome, SeriesOutcome::Restored(_)));
        assert_eq!(helper.starts.load(Ordering::SeqCst), 2, "вторая попытка");
        assert_eq!(*attempts.lock().unwrap(), vec![1, 2]);
        let snapshot = state.reconnect_snapshot();
        assert_eq!(snapshot.attempt, 0, "успех сбрасывает счётчик");
        assert!(snapshot.exhausted.is_none());
        let infos: Vec<_> = state
            .logs
            .list(Some(LogLevel::Info), 20)
            .into_iter()
            .map(|entry| entry.message)
            .collect();
        assert!(infos
            .iter()
            .any(|message| message.contains("попытка 2 из 3")));
        assert!(infos
            .iter()
            .any(|message| message.contains("подключение восстановлено")));
    }

    #[test]
    fn exhausted_series_keeps_protection_and_waits_for_manual_action() {
        fast_delay();
        let helper = Arc::new(AlwaysFailingHelper::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);

        let outcome = run_series(&state, no_progress);
        let error = match outcome {
            SeriesOutcome::Exhausted(error) => error,
            _ => panic!("серия должна исчерпаться"),
        };
        assert_eq!(error.code, "RECONNECT_FAILED");
        assert_eq!(
            helper.starts.load(Ordering::SeqCst),
            MAX_ATTEMPTS,
            "ровно три попытки"
        );
        assert_eq!(
            helper.inner.stop_calls(),
            0,
            "fail-closed: серия не снимает правила kill switch"
        );
        let snapshot = state.reconnect_snapshot();
        assert_eq!(
            snapshot.exhausted.as_ref().map(|error| error.code.as_str()),
            Some("RECONNECT_FAILED"),
            "причина ждёт ручного действия"
        );
        let warnings = state.logs.list(Some(LogLevel::Warn), 20);
        assert!(warnings
            .iter()
            .any(|entry| entry.message.contains("правила kill switch сохраняются")));
    }

    #[test]
    fn exhausted_series_is_not_repeated_until_manual_action() {
        fast_delay();
        let helper = Arc::new(AlwaysFailingHelper::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);

        run_series(&state, no_progress);
        let outcome = run_series(&state, no_progress);
        assert!(matches!(outcome, SeriesOutcome::Cancelled));
        assert_eq!(
            helper.starts.load(Ordering::SeqCst),
            MAX_ATTEMPTS,
            "повторных попыток без ручного действия нет"
        );
    }

    /// Ручное отключение отменяет серию: attempts сброшены, стартов нет.
    #[test]
    fn manual_disconnect_cancels_series() {
        let helper = Arc::new(AlwaysFailingHelper::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);
        state.reconnect_disarm();

        let outcome = run_series(&state, no_progress);
        assert!(matches!(outcome, SeriesOutcome::Cancelled));
        assert_eq!(helper.starts.load(Ordering::SeqCst), 0);
        let snapshot = state.reconnect_snapshot();
        assert_eq!(snapshot.attempt, 0);
        assert!(snapshot.exhausted.is_none());
    }

    /// Отмена серии посреди паузы тоже даёт `Cancelled`: попытка 1 выполнена,
    /// остальные не тратятся, состояние серии сброшено отключением.
    #[test]
    fn cancel_during_pause_stops_remaining_attempts() {
        fast_delay();
        let helper = Arc::new(AlwaysFailingHelper::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);

        // Отключение — из прогресс-колбэка первой попытки: как будто команда
        // `disconnect` пришла, пока серия уже работала.
        let outcome = run_series(&state, |_| state.reconnect_disarm());
        assert!(matches!(outcome, SeriesOutcome::Cancelled));
        assert_eq!(helper.starts.load(Ordering::SeqCst), 1);
        let snapshot = state.reconnect_snapshot();
        assert_eq!(snapshot.attempt, 0);
        assert!(snapshot.exhausted.is_none());
    }

    /// Ручное действие сбрасывает исчерпание: отключение — сразу, успешный
    /// старт — через общий `connect_inner` (он вызывает `reconnect_arm`).
    #[test]
    fn manual_actions_reset_exhausted_state() {
        fast_delay();
        let helper = Arc::new(AlwaysFailingHelper::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);
        run_series(&state, no_progress);
        assert!(state.reconnect_snapshot().exhausted.is_some());

        state.reconnect_disarm();
        let snapshot = state.reconnect_snapshot();
        assert!(snapshot.exhausted.is_none(), "отключение сбрасывает");
        assert!(!state.reconnect_watching());

        // Успешное ручное подключение (mock стартует) снова открывает серию.
        let working = Arc::new(MockHelperClient::new());
        let state = state_with(working.clone());
        arm_connected_session(&state);
        run_series(&state, no_progress);
        let snapshot = state.reconnect_snapshot();
        assert!(snapshot.exhausted.is_none());
        assert!(state.reconnect_watching());
    }

    /// Отменённая серия (ручное отключение) не блокирует следующую: guard
    /// серии обязан сниматься при любом выходе, иначе после переподключения
    /// падения ядра больше не детектируются.
    #[test]
    fn cancelled_series_does_not_block_future_series() {
        fast_delay();
        let helper = Arc::new(AlwaysFailingHelper::new());
        let mut state = state_with(helper.clone());
        arm_connected_session(&state);

        // Первая серия отменена ручным отключением.
        state.reconnect_disarm();
        assert!(matches!(
            run_series(&state, no_progress),
            SeriesOutcome::Cancelled
        ));

        // Пользователь подключился снова, ядро упало ещё раз: серия работает.
        let working = Arc::new(MockHelperClient::new());
        state.helper = working.clone();
        state.reconnect_arm();
        let outcome = run_series(&state, no_progress);
        assert!(matches!(outcome, SeriesOutcome::Restored(_)));
        assert_eq!(
            working.start_calls(),
            1,
            "новая серия стартовала ядро, guard прошлой серии не мешает"
        );
    }

    /// Мьютекс не удерживается через `connect_inner`: колбэк прогресса может
    /// читать состояние — deadlock был бы признаком удержания замка.
    #[test]
    fn progress_callback_runs_without_holding_lock() {
        let helper = Arc::new(MockHelperClient::new());
        let state = state_with(helper.clone());
        arm_connected_session(&state);

        let observed = Mutex::new(Vec::new());
        let outcome = run_series(&state, |snapshot| {
            observed
                .lock()
                .unwrap()
                .push((state.reconnect_watching(), snapshot.attempt));
        });
        assert!(matches!(outcome, SeriesOutcome::Restored(_)));
        assert_eq!(*observed.lock().unwrap(), vec![(true, 1)]);
    }
}
