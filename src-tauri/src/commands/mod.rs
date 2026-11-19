//! Тонкие Tauri commands: без бизнес-логики в UI, только вызов ядра.
//!
//! UI не формирует конфиг sing-box: подключение собирается здесь из сохранённых
//! настроек, выбранного профиля и правил allowlist (PRD §6.2).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use splitroute_protocol::{
    AppRule, AppRuleView, CoreState, ErrorCode, HelperStatus, InstalledApp, LogLevel, Profile,
    RunningApp,
};
use tauri::Manager;

use crate::catalog;
use crate::config::{build_config, ConfigError};
use crate::latency::{self, LatencyProbe, LatencyTracker, PingResult, SwitchPolicy};
use crate::logging::{LogBuffer, LogEntry};
use crate::privileged::{HelperClient, HelperClientError, HelperInstaller, InstallerError};
use crate::rules;
use crate::store::{now_secs, AppSettings, Store, StoreError, SubscriptionState, UpdateChannel};
use crate::subscription::{
    fetch_and_parse, FetchError, FetchRequest, SkippedProfile, SubscriptionFetcher,
};
use crate::traffic::TrafficEndpoint;
use crate::updates::{self, UpdateCheck, UpdateError, UpdateStatus};

/// Ошибка команды: стабильный код + русский текст для пользователя.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
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
            HelperClientError::Helper { code, .. } => match code {
                // Права/неподдерживаемая ОС: UI показывает отдельную причину.
                ErrorCode::KillSwitchFailed => "KILL_SWITCH_FAILED",
                _ => "HELPER_ERROR",
            },
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

impl From<UpdateError> for CommandError {
    fn from(error: UpdateError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

/// Параметры и состояние статистики текущей сессии.
#[derive(Default)]
struct TrafficState {
    /// Порт и secret действующего ядра; `None` — сессии нет.
    endpoint: Option<TrafficEndpoint>,
    /// Предупреждение об ошибке уже записано: не спамим лог каждый опрос.
    error_logged: bool,
}

/// Код ошибки: загрузка этой подписки уже идёт. Параллельный запрос
/// отклоняется, чтобы ручное обновление и планировщик не дублировали сетевую
/// операцию. Обновления разных подписок друг другу не мешают.
pub(crate) const FETCH_IN_PROGRESS: &str = "FETCH_IN_PROGRESS";

/// Код ошибки: таска фоновой загрузки погибла (паника в `spawn_blocking`).
/// Случай «не должно случаться» — защитная ветка на месте JoinError.
pub(crate) const FETCH_ABORTED: &str = "FETCH_ABORTED";

/// Право на одну загрузку подписки. Идентификатор снимается при `Drop`,
/// поэтому ошибка или паника не оставляют подписку заблокированной навсегда.
pub(crate) struct FetchGuard<'a> {
    busy: &'a Mutex<HashSet<String>>,
    id: String,
}

impl Drop for FetchGuard<'_> {
    fn drop(&mut self) {
        self.busy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
    }
}

/// Состояние приложения, доступное командам.
pub struct AppState {
    pub helper: Arc<dyn HelperClient>,
    pub installer: Arc<dyn HelperInstaller>,
    pub store: Store,
    pub fetcher: Arc<dyn SubscriptionFetcher>,
    pub logs: LogBuffer,
    traffic: Mutex<TrafficState>,
    probe: Arc<dyn LatencyProbe>,
    latency: Mutex<LatencyTracker>,
    /// Ошибка автоподключения при запуске (B2): UI показывает её, пока не
    /// пройдёт удачное подключение или отключение.
    startup_error: Mutex<Option<CommandError>>,
    /// Идущие загрузки: не больше одной на подписку, разные подписки
    /// обновляются параллельно.
    fetching: Mutex<HashSet<String>>,
    /// Время последней попытки обновления каждой подписки (unix, секунды).
    /// В памяти: после перезапуска расписание снова отсчитывается от
    /// `fetched_at` записи.
    last_fetch_attempt: Mutex<HashMap<String, u64>>,
}

impl AppState {
    pub fn new(
        helper: Arc<dyn HelperClient>,
        installer: Arc<dyn HelperInstaller>,
        store: Store,
        fetcher: Arc<dyn SubscriptionFetcher>,
    ) -> Self {
        Self::with_probe(
            helper,
            installer,
            store,
            fetcher,
            Arc::new(latency::TcpProbe),
        )
    }

    fn with_probe(
        helper: Arc<dyn HelperClient>,
        installer: Arc<dyn HelperInstaller>,
        store: Store,
        fetcher: Arc<dyn SubscriptionFetcher>,
        probe: Arc<dyn LatencyProbe>,
    ) -> Self {
        Self {
            helper,
            installer,
            store,
            fetcher,
            logs: LogBuffer::default(),
            traffic: Mutex::new(TrafficState::default()),
            probe,
            latency: Mutex::new(LatencyTracker::default()),
            startup_error: Mutex::new(None),
            fetching: Mutex::new(HashSet::new()),
            last_fetch_attempt: Mutex::new(HashMap::new()),
        }
    }

    /// Тестовая подмена источника задержек: сеть в юнит-тестах не нужна.
    #[cfg(test)]
    pub(crate) fn with_test_probe(self, probe: Arc<dyn LatencyProbe>) -> Self {
        Self { probe, ..self }
    }

    fn lock_latency(&self) -> std::sync::MutexGuard<'_, LatencyTracker> {
        self.latency
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn reset_latency(&self) {
        self.lock_latency().reset();
    }

    /// Новый `start` — новая сессия: старые счётчики недоступны.
    fn set_traffic_endpoint(&self, endpoint: Option<TrafficEndpoint>) {
        let mut traffic = self.lock_traffic();
        traffic.endpoint = endpoint;
        traffic.error_logged = false;
    }

    fn traffic_endpoint(&self) -> Option<TrafficEndpoint> {
        self.lock_traffic().endpoint.clone()
    }

    /// `true` — это первый сбой подряд, предупреждение нужно записать.
    fn note_traffic_error(&self) -> bool {
        let mut traffic = self.lock_traffic();
        if traffic.error_logged {
            return false;
        }
        traffic.error_logged = true;
        true
    }

    fn clear_traffic_error(&self) {
        self.lock_traffic().error_logged = false;
    }

    fn lock_traffic(&self) -> std::sync::MutexGuard<'_, TrafficState> {
        self.traffic
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Запоминает ошибку автоподключения для UI (B2).
    pub(crate) fn set_startup_error(&self, error: Option<CommandError>) {
        *self
            .startup_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = error;
    }

    pub(crate) fn startup_error(&self) -> Option<CommandError> {
        self.startup_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Захватывает право на загрузку подписки. `None` — эта подписка уже
    /// обновляется. Разные подписки друг другу не мешают.
    pub(crate) fn try_begin_fetch(&self, id: &str) -> Option<FetchGuard<'_>> {
        let mut busy = self
            .fetching
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if busy.contains(id) {
            return None;
        }
        busy.insert(id.to_string());
        Some(FetchGuard {
            busy: &self.fetching,
            id: id.to_string(),
        })
    }

    pub(crate) fn last_fetch_attempt(&self, id: &str) -> Option<u64> {
        self.last_fetch_attempt
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(id)
            .copied()
    }

    pub(crate) fn set_last_fetch_attempt(&self, id: &str, at: u64) {
        self.last_fetch_attempt
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.to_string(), at);
    }

    /// Когда ждать следующее автообновление этой подписки (M1, PRD §6.1).
    pub(crate) fn next_update_at(&self, subscription: &SubscriptionState) -> Option<u64> {
        crate::autoupdate::next_update_at(
            subscription.fetched_at,
            self.last_fetch_attempt(&subscription.id),
            subscription.auto_update_hours,
        )
    }
}

/// Счётчики трафика сессии в байтах. `available=false` — источник недоступен.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TrafficStats {
    pub available: bool,
    pub upload_total: u64,
    pub download_total: u64,
}

impl TrafficStats {
    fn unavailable() -> Self {
        Self {
            available: false,
            upload_total: 0,
            download_total: 0,
        }
    }
}

/// Одна подписка для UI: параметры, расписание и кэш локаций.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub user_agent: String,
    pub auto_update_hours: u16,
    pub fetched_at: Option<u64>,
    /// Когда произойдёт следующее автообновление (M1); `null` — не запланировано.
    pub next_update_at: Option<u64>,
    pub profiles: Vec<Profile>,
}

/// Профили и выбранная локация для экрана «Локации».
#[derive(Debug, Clone, Serialize)]
pub struct ProfilesResponse {
    /// Локации, сгруппированные по подпискам в порядке добавления.
    pub subscriptions: Vec<SubscriptionView>,
    /// Все локации плоским списком: задержки и активная локация не зависят
    /// от того, к какой подписке они относятся.
    pub profiles: Vec<Profile>,
    pub selected_profile_id: Option<String>,
}

/// Результат обновления одной подписки.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionFetchResponse {
    pub subscription_id: String,
    pub name: String,
    pub profiles: Vec<Profile>,
    pub skipped: Vec<SkippedProfile>,
    pub selected_profile_id: Option<String>,
    pub fetched_at: u64,
    pub next_update_at: Option<u64>,
}

/// Результат проверки задержек для экрана «Локации».
#[derive(Debug, Clone, Serialize)]
pub struct LatencyResponse {
    pub results: Vec<PingResult>,
    pub best_profile_id: Option<String>,
    /// Локация, на которую стоит переключиться: только при включённом
    /// автопереключении и активном подключении.
    pub switch_to: Option<String>,
    pub checked_at: u64,
}

/// Проекция записи подписки для UI: без `raw` профилей, с расписанием.
fn subscription_view(state: &AppState, subscription: &SubscriptionState) -> SubscriptionView {
    SubscriptionView {
        id: subscription.id.clone(),
        name: subscription.name.clone(),
        url: subscription.url.clone(),
        user_agent: subscription.user_agent.clone(),
        auto_update_hours: subscription.auto_update_hours,
        fetched_at: subscription.fetched_at,
        next_update_at: state.next_update_at(subscription),
        profiles: subscription
            .profiles
            .iter()
            .cloned()
            .map(sanitize_profile)
            .collect(),
    }
}

fn profiles_response(state: &AppState) -> ProfilesResponse {
    let data = state.store.data();
    ProfilesResponse {
        subscriptions: data
            .subscriptions
            .iter()
            .map(|subscription| subscription_view(state, subscription))
            .collect(),
        profiles: data
            .all_profiles()
            .into_iter()
            .map(sanitize_profile)
            .collect(),
        selected_profile_id: data.selected_profile_id,
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
    let profile = data.find_profile(selected).ok_or_else(|| {
        CommandError::new(
            "PROFILE_MISSING",
            "Выбранная локация отсутствует в подписке.",
        )
    })?;

    // Статистика не должна быть условием подключения: без свободного порта
    // туннель всё равно поднимается, а UI показывает «—».
    let traffic = match TrafficEndpoint::allocate() {
        Ok(endpoint) => Some(endpoint),
        Err(error) => {
            state.logs.push(
                LogLevel::Warn,
                format!("Статистика трафика недоступна: {error}"),
            );
            None
        }
    };
    let config = build_config(
        &data.settings.network,
        profile,
        &data.rules,
        traffic.as_ref().map(TrafficEndpoint::api),
    )?;
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
    let kill_switch = data.settings.network.kill_switch;
    if kill_switch {
        state.logs.push(
            LogLevel::Info,
            "Kill switch: включён, прямой выход блокируется helper'ом",
        );
    }
    let status = state.helper.start(config, kill_switch)?;
    // Новая сессия начинается с нулевых счётчиков: порт и secret новые.
    state.set_traffic_endpoint(traffic);
    // Успешное подключение закрывает ошибку автоподключения при запуске (B2).
    state.set_startup_error(None);
    state
        .logs
        .push(LogLevel::Info, format!("Подключено: {}", profile.name));
    Ok(status)
}

pub(crate) fn disconnect_inner(state: &AppState) -> Result<HelperStatus, CommandError> {
    let status = state.helper.stop()?;
    // После `stop` статистика недоступна и UI показывает нули/«—».
    state.set_traffic_endpoint(None);
    state.set_startup_error(None);
    state.logs.push(LogLevel::Info, "Подключение остановлено");
    state.reset_latency();
    Ok(status)
}

/// Замеряет задержки всех локаций и обновляет состояние автопереключения.
///
/// В логи попадают только имена локаций и числа: адреса серверов не пишутся.
pub(crate) fn profiles_ping_inner(state: &AppState) -> LatencyResponse {
    let data = state.store.data();
    let auto = data.settings.preferences.auto_select.clone();
    let profiles = data.all_profiles();
    let results = latency::measure(state.probe.as_ref(), &profiles, latency::PROBE_TIMEOUT);
    let best = latency::best_profile(&results).cloned();

    let running = state
        .helper
        .status()
        .map(|status| status.state == CoreState::Running)
        .unwrap_or(false);
    let switch_to = if auto.auto_switch && running {
        let policy = SwitchPolicy {
            degradation_ms: auto.degradation_ms,
            confirmations: auto.confirmations,
            failures_to_switch: auto.failures_to_switch,
        };
        state
            .lock_latency()
            .observe(data.selected_profile_id.as_deref(), &results, policy)
    } else {
        // Выключенная настройка не должна накапливать серии и «дёргать» туннель
        // сразу после включения.
        state.reset_latency();
        None
    };

    let names: std::collections::HashMap<&str, &str> = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.name.as_str()))
        .collect();
    for result in &results {
        let Some(name) = names.get(result.profile_id.as_str()) else {
            continue;
        };
        match result.latency_ms {
            Some(latency_ms) => state.logs.push(
                LogLevel::Debug,
                format!("Задержка: {name} — {latency_ms} мс"),
            ),
            None => state.logs.push(
                LogLevel::Debug,
                format!(
                    "Задержка: {name} — ошибка ({})",
                    result
                        .error
                        .unwrap_or(latency::PingErrorCode::Unreachable)
                        .as_str()
                ),
            ),
        }
    }

    let available = results
        .iter()
        .filter(|result| result.latency_ms.is_some())
        .count();
    state.logs.push(
        LogLevel::Info,
        format!(
            "Проверка задержек: доступно {available} из {}",
            results.len()
        ),
    );
    if let Some(best) = &best {
        if let Some(name) = names.get(best.profile_id.as_str()) {
            state.logs.push(
                LogLevel::Info,
                format!(
                    "Лучшая локация: {name} — {} мс",
                    best.latency_ms.unwrap_or_default()
                ),
            );
        }
    }

    LatencyResponse {
        results,
        best_profile_id: best.map(|result| result.profile_id),
        switch_to,
        checked_at: now_secs(),
    }
}

/// Переключает локацию с переподключением (PRD §6.1).
///
/// Подключённое состояние меняется только через `stop` → выбор → `start` с тем
/// же флагом kill switch. Если новый старт не удался, приложение возвращается к
/// прежней локации: сеть не остаётся без туннеля, а правила kill switch — в
/// неожиданном состоянии.
pub(crate) fn switch_profile_inner(
    state: &AppState,
    id: &str,
) -> Result<HelperStatus, CommandError> {
    let data = state.store.data();
    let target = data.find_profile(id).ok_or_else(|| {
        CommandError::new(
            "PROFILE_MISSING",
            "Выбранная локация отсутствует в подписке.",
        )
    })?;
    let target_name = target.name.clone();
    let previous = data.selected_profile_id.clone();
    let previous_name = previous
        .as_deref()
        .and_then(|id| data.find_profile(id))
        .map(|profile| profile.name.clone());
    let status = state.helper.status()?;

    if previous.as_deref() == Some(id) {
        state.store.select_profile(id)?;
        return Ok(status);
    }
    // Без активного туннеля переключение — это только сохранение выбора.
    if status.state != CoreState::Running {
        state.store.select_profile(id)?;
        state.reset_latency();
        return Ok(status);
    }

    state.helper.stop()?;
    state.set_traffic_endpoint(None);
    state.store.select_profile(id)?;
    state.reset_latency();

    match connect_inner(state) {
        Ok(status) => {
            if let Some(previous_name) = previous_name {
                state.logs.push(
                    LogLevel::Info,
                    format!("Переключение локации: {previous_name} → {target_name}"),
                );
            }
            Ok(status)
        }
        Err(error) => {
            if let Some(previous) = previous {
                let _ = state.store.select_profile(&previous);
                match connect_inner(state) {
                    Ok(_) => state.logs.push(
                        LogLevel::Warn,
                        format!(
                            "Не удалось переключиться на {target_name}: \
                             восстановлена прежняя локация"
                        ),
                    ),
                    Err(_) => state.logs.push(
                        LogLevel::Warn,
                        format!(
                            "Не удалось переключиться на {target_name}: \
                             прежняя локация не запустилась"
                        ),
                    ),
                }
            }
            Err(error)
        }
    }
}

/// Читает счётчики у текущей сессии. Ошибка не влияет на подключение: в лог
/// попадает одно предупреждение, UI получает недоступное состояние.
pub(crate) fn traffic_stats_inner(state: &AppState) -> TrafficStats {
    let Some(endpoint) = state.traffic_endpoint() else {
        return TrafficStats::unavailable();
    };
    match endpoint.totals() {
        Ok(totals) => {
            state.clear_traffic_error();
            TrafficStats {
                available: true,
                upload_total: totals.upload,
                download_total: totals.download,
            }
        }
        Err(error) => {
            if state.note_traffic_error() {
                state.logs.push(
                    LogLevel::Warn,
                    format!("Статистика трафика недоступна: {error}"),
                );
            }
            TrafficStats::unavailable()
        }
    }
}

/// Опрос локального Clash API: блокирующий reqwest, поэтому тело выполняется в
/// `spawn_blocking`. Прямое исполнение в таске runtime паникует: blocking-клиент
/// reqwest в debug-сборке дропает свой внутренний runtime, а tokio запрещает
/// дропать runtime из async-контекста — таска команды умирает и промис в UI не
/// разрешается.
#[tauri::command]
pub async fn traffic_stats(app: tauri::AppHandle) -> TrafficStats {
    tauri::async_runtime::spawn_blocking(move || traffic_stats_inner(&app.state::<AppState>()))
        .await
        .unwrap_or_else(|_| TrafficStats::unavailable())
}

/// Загружает одну подписку по её идентификатору.
///
/// Используется и вручную, и планировщиком автообновления, поэтому попытка
/// запоминается здесь для обоих путей: следующее автообновление подписки
/// отодвигается на интервал после любой попытки (PRD §6.1). `now` передаётся
/// явно: планировщик считает расписание от своего «сейчас», тесты не зависят
/// от системных часов.
pub(crate) fn subscription_fetch_inner(
    state: &AppState,
    id: &str,
    now: u64,
) -> Result<SubscriptionFetchResponse, CommandError> {
    // Одна загрузка на подписку: ручное обновление и планировщик не дублируют
    // запрос. Обновления разных подписок друг другу не мешают. Guard с `Drop`
    // живёт до конца функции: параллельный запрос отклоняется и при ошибке.
    let _guard = state.try_begin_fetch(id).ok_or_else(|| {
        CommandError::new(FETCH_IN_PROGRESS, "Обновление подписки уже выполняется.")
    })?;

    let subscription = state
        .store
        .subscription(id)
        .ok_or_else(|| CommandError::new("SUBSCRIPTION_NOT_FOUND", "Подписка не найдена."))?;
    if subscription.url.trim().is_empty() {
        return Err(FetchError::MissingUrl.into());
    }

    let request = FetchRequest {
        url: subscription.url.clone(),
        hwid: state.store.settings().subscription.hwid,
        user_agent: subscription.user_agent.clone(),
    };

    state.logs.push(
        LogLevel::Info,
        format!(
            "Обновление подписки: {}",
            subscription_display_name(&subscription)
        ),
    );
    match fetch_and_parse(state.fetcher.as_ref(), &request) {
        Ok(parsed) => {
            let fetched_at = now_secs();
            state
                .store
                .set_subscription_profiles(id, parsed.profiles.clone(), fetched_at)?;
            state.set_last_fetch_attempt(id, now);
            state.logs.push(
                LogLevel::Info,
                format!(
                    "Подписка обновлена: {} — профилей {}",
                    subscription_display_name(&subscription),
                    parsed.profiles.len()
                ),
            );
            Ok(SubscriptionFetchResponse {
                subscription_id: id.to_string(),
                name: subscription.name,
                profiles: parsed.profiles.into_iter().map(sanitize_profile).collect(),
                skipped: parsed.skipped,
                selected_profile_id: state.store.data().selected_profile_id,
                fetched_at,
                next_update_at: state
                    .store
                    .subscription(id)
                    .and_then(|updated| state.next_update_at(&updated)),
            })
        }
        Err(error) => {
            // Попытка запоминается и при ошибке: иначе сломанный провайдер
            // получал бы запрос на каждой проверке расписания.
            state.set_last_fetch_attempt(id, now);
            state
                .logs
                .push(LogLevel::Warn, format!("Ошибка подписки: {}", error.code()));
            Err(error.into())
        }
    }
}

/// Отображаемое имя подписки для логов: имя задаёт пользователь и секретом
/// не является, поэтому пустое имя не превращается в URL.
fn subscription_display_name(subscription: &SubscriptionState) -> String {
    if subscription.name.trim().is_empty() {
        "без имени".to_string()
    } else {
        subscription.name.trim().to_string()
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
        .and_then(|id| data.find_profile(id))
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| "не выбрана".to_string());
    let profiles = data.all_profiles();
    let fetched = data
        .subscriptions
        .iter()
        .filter_map(|subscription| subscription.fetched_at)
        .max()
        .map(|time| format!("обновлена {time}"))
        .unwrap_or_else(|| "нет данных".to_string());

    format!(
        "SplitRoute {}\nОС: {} ({})\nHelper: {}\nЛокация: {}\nПодписок: {} ({}, профилей {})\nПравил: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        helper,
        location,
        data.subscriptions.len(),
        fetched,
        profiles.len(),
        data.rules.len()
    )
}

#[tauri::command]
pub fn helper_status(state: tauri::State<'_, AppState>) -> Result<HelperStatus, CommandError> {
    Ok(state.helper.status()?)
}

/// Ошибка автоподключения при запуске (B2) или `null`, если её не было.
/// Сбрасывается успешным `connect`/`disconnect`: UI не держит устаревшую причину.
#[tauri::command]
pub fn autoconnect_status(state: tauri::State<'_, AppState>) -> Option<CommandError> {
    state.startup_error()
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

/// Ручное обновление одной подписки.
///
/// Блокирующий запрос к провайдеру (до 45 с) выполняется в `spawn_blocking`:
/// `#[tauri::command(async)]` на синхронном теле ставит его в таску runtime,
/// где blocking-reqwest паникует при освобождении своего внутреннего runtime
/// (tokio запрещает дроп runtime из async-контекста). Из-за этого промис в UI
/// не разрешался — индикатор обновления крутится вечно.
#[tauri::command]
pub async fn subscription_fetch(
    app: tauri::AppHandle,
    id: String,
) -> Result<SubscriptionFetchResponse, CommandError> {
    let response = tauri::async_runtime::spawn_blocking(move || {
        subscription_fetch_inner(&app.state::<AppState>(), &id, now_secs())
    })
    .await
    .map_err(|error| {
        CommandError::new(FETCH_ABORTED, format!("фоновая задача прервана: {error}"))
    })?;
    response
}

/// Список подписок с расписанием и кэшем локаций.
#[tauri::command]
pub fn subscriptions_list(state: tauri::State<'_, AppState>) -> Vec<SubscriptionView> {
    state
        .store
        .subscriptions()
        .iter()
        .map(|subscription| subscription_view(&state, subscription))
        .collect()
}

/// Добавляет подписку. Загрузку после добавления запускает UI: команда не
/// должна тянуть сеть под блокировкой стора дольше, чем нужно.
#[tauri::command]
pub fn subscription_add(
    state: tauri::State<'_, AppState>,
    name: String,
    url: String,
    user_agent: String,
    auto_update_hours: u16,
) -> Result<SubscriptionView, CommandError> {
    let entry = state
        .store
        .add_subscription(&name, &url, &user_agent, auto_update_hours)?;
    state.logs.push(
        LogLevel::Info,
        format!("Подписка добавлена: {}", subscription_display_name(&entry)),
    );
    Ok(subscription_view(&state, &entry))
}

/// Обновляет параметры подписки. Смена URL сбрасывает кэш этой подписки.
#[tauri::command]
pub fn subscription_update(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    url: String,
    user_agent: String,
    auto_update_hours: u16,
) -> Result<SubscriptionView, CommandError> {
    let entry =
        state
            .store
            .update_subscription(&id, &name, &url, &user_agent, auto_update_hours)?;
    Ok(subscription_view(&state, &entry))
}

/// Удаляет подписку вместе с кэшем. Активный туннель не затрагивается.
#[tauri::command]
pub fn subscription_delete(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<ProfilesResponse, CommandError> {
    state.store.delete_subscription(&id)?;
    Ok(profiles_response(&state))
}

#[tauri::command]
pub fn profiles_list(state: tauri::State<'_, AppState>) -> ProfilesResponse {
    profiles_response(&state)
}

#[tauri::command]
pub fn profile_select(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<ProfilesResponse, CommandError> {
    state.store.select_profile(&id)?;
    Ok(profiles_response(&state))
}

/// Проверка задержек не должна блокировать окно: TCP-connect с таймаутом и
/// параллельными замерами выполняется в thread pool Tauri.
#[tauri::command(async)]
pub fn profiles_ping(state: tauri::State<'_, AppState>) -> LatencyResponse {
    profiles_ping_inner(&state)
}

/// Переключение локации с переподключением. Без активного туннеля меняется
/// только сохранённый выбор; подключённое состояние проходит через `stop`/`start`.
#[tauri::command]
pub fn profile_switch(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<ProfilesResponse, CommandError> {
    let status = switch_profile_inner(&state, &id)?;
    crate::tray::publish(&app, &status);
    Ok(profiles_response(&state))
}

/// Список правил allowlist с признаком отсутствующего пути (PRD §6.3).
/// `async`: проверка путей — локальный `stat`, он не должен идти на главном
/// потоке окна, если правил много или диск медленный.
#[tauri::command(async)]
pub fn rules_list(state: tauri::State<'_, AppState>) -> Vec<AppRuleView> {
    rules_list_inner(&state)
}

pub(crate) fn rules_list_inner(state: &AppState) -> Vec<AppRuleView> {
    rules::views(state.store.rules())
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

/// Список запущенных процессов текущей ОС (S4-T11). `async`: обход процессов
/// может занять десятки миллисекунд и не должен идти на главном потоке окна.
/// Ошибки чтения не ломают вкладку: пустой список и причина в Debug-логе.
#[tauri::command(async)]
pub fn apps_running(state: tauri::State<'_, AppState>) -> Vec<RunningApp> {
    apps_running_inner(&state)
}

pub(crate) fn apps_running_inner(state: &AppState) -> Vec<RunningApp> {
    let report = catalog::running::scan();
    for reason in &report.skipped {
        state
            .logs
            .push(LogLevel::Debug, format!("Запущенные: {reason}"));
    }
    if report.system_skipped > 0 {
        state.logs.push(
            LogLevel::Debug,
            format!(
                "Запущенные: пропущено системных процессов: {}",
                report.system_skipped
            ),
        );
    }
    if report.truncated > 0 {
        state.logs.push(
            LogLevel::Debug,
            format!("Запущенные: список ограничен, скрыто: {}", report.truncated),
        );
    }
    state.logs.push(
        LogLevel::Info,
        format!("Запущенные процессы: найдено {}", report.apps.len()),
    );
    report.apps
}

/// Правило из запущенного процесса: macOS — `path_regex` по `.app`, если путь
/// внутри бандла, иначе `process_path`; Windows/Linux — `process_path`.
#[tauri::command]
pub fn app_rule_from_running(app: RunningApp) -> AppRule {
    catalog::running::rule_from_running(&app)
}

/// Иконка приложения как data URL (S4-T2). `async`, чтобы чтение `.icns`/`.exe`
/// и декодирование PNG не выполнялись на главном потоке: в Tauri 2 синхронные
/// команды идут на main thread. `None` — штатный fallback без иконки.
#[tauri::command(async)]
pub fn app_icon(app: InstalledApp) -> Option<String> {
    catalog::icons::icon_for(&app)
}

#[tauri::command(async)]
pub fn rules_upsert(
    state: tauri::State<'_, AppState>,
    rule: AppRule,
) -> Result<Vec<AppRuleView>, CommandError> {
    let rules = state.store.upsert_rule(rule)?;
    Ok(rules::views(rules))
}

#[tauri::command(async)]
pub fn rules_delete(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<AppRuleView>, CommandError> {
    let rules = state.store.delete_rule(&id)?;
    Ok(rules::views(rules))
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
    // Язык — единое состояние в Rust; меню трея перерисовывается сразу.
    crate::tray::set_language(&app, saved.preferences.language);
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

/// Состояние канала обновлений: текущая версия, канал и настроен ли домен.
#[tauri::command]
pub fn update_status(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> UpdateStatus {
    updates::status(&app, &state.store.settings().preferences.updates)
}

/// Проверка обновлений. `async`: сетевой запрос с таймаутом не должен
/// выполняться на главном потоке окна.
#[tauri::command]
pub async fn update_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    channel: UpdateChannel,
) -> Result<UpdateCheck, CommandError> {
    let result = updates::check(&app, channel).await;
    match &result {
        Ok(check) if check.available => state.logs.push(
            LogLevel::Info,
            format!(
                "Обновление доступно: {}",
                check.version.as_deref().unwrap_or("—")
            ),
        ),
        Ok(_) => state.logs.push(LogLevel::Info, "Обновлений нет"),
        Err(error) => state.logs.push(
            LogLevel::Warn,
            format!("Проверка обновлений: {}", error.code()),
        ),
    }
    result.map_err(Into::into)
}

/// Скачивание и установка обновления. Ядро останавливается только после
/// успешной проверки: ошибка сети или подписи не рвёт работающий туннель.
#[tauri::command]
pub async fn update_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    channel: UpdateChannel,
) -> Result<(), CommandError> {
    state.logs.push(
        LogLevel::Info,
        format!("Установка обновления: канал {}", channel.as_str()),
    );
    let helper = state.helper.clone();
    let logs = &state.logs;
    updates::install(&app, channel, move || {
        if let Err(error) = helper.stop() {
            logs.push(
                LogLevel::Warn,
                format!("Перед обновлением не удалось остановить ядро: {error}"),
            );
        } else {
            logs.push(
                LogLevel::Info,
                "Перед обновлением ядро и правила kill switch остановлены",
            );
        }
    })
    .await
    .map_err(|error| {
        state.logs.push(
            LogLevel::Warn,
            format!("Установка обновления: {}", error.code()),
        );
        CommandError::from(error)
    })
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
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Одноразовый локальный HTTP-сервер вместо Clash API.
    fn spawn_traffic_api(body: &'static str) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (port, handle)
    }

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

    /// Создаёт (один раз) подписку с тестовым URL и загружает её. Повторный
    /// вызов обновляет ту же подписку: так проверяется и повторная загрузка.
    fn fetch_sub(state: &AppState) -> Result<SubscriptionFetchResponse, CommandError> {
        let id = match state.store.subscriptions().first() {
            Some(existing) => existing.id.clone(),
            None => {
                state
                    .store
                    .add_subscription("", "https://example.invalid/sub", "", 12)
                    .unwrap()
                    .id
            }
        };
        subscription_fetch_inner(state, &id, 1_800_000_000)
    }

    #[test]
    fn fetch_success_caches_profiles_and_selects_first() {
        let state = state_with(Ok(fixture()));
        let result = fetch_sub(&state).unwrap();
        assert_eq!(result.profiles.len(), 2);
        assert_eq!(
            result.selected_profile_id.as_deref(),
            Some(result.profiles[0].id.as_str())
        );
        assert_eq!(state.store.data().all_profiles().len(), 2);
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
        fetch_sub(&state).unwrap();
        assert_eq!(state.store.data().all_profiles().len(), 2);

        let error = fetch_sub(&state).unwrap_err();
        assert_eq!(error.code, "NETWORK");
        assert_eq!(
            state.store.data().all_profiles().len(),
            2,
            "кэш последнего успешного ответа сохраняется"
        );
    }

    #[test]
    fn responses_do_not_expose_raw_profile_json() {
        let state = state_with(Ok(fixture()));
        let response = fetch_sub(&state).unwrap();
        assert!(response
            .profiles
            .iter()
            .all(|profile| profile.raw.is_none()));
        assert!(profiles_response(&state)
            .profiles
            .iter()
            .all(|profile| profile.raw.is_none()));
        assert!(state
            .store
            .data()
            .all_profiles()
            .iter()
            .all(|profile| profile.raw.is_some()));
    }

    /// Подписка без URL не создаётся (store отвергает пустой URL), поэтому
    /// загрузка несуществующей записи — типизированная ошибка, а не паника.
    #[test]
    fn fetch_unknown_subscription_is_typed_error() {
        let state = state_with(Ok(fixture()));
        let error = subscription_fetch_inner(&state, "missing", 1_800_000_000).unwrap_err();
        assert_eq!(error.code, "SUBSCRIPTION_NOT_FOUND");
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
        fetch_sub(&state).unwrap();
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
        settings.subscription.hwid = "0123456789abcdef".to_string();
        settings_update_inner(&state, settings).unwrap();
        // URL подписки живёт в записи и в диагностику не попадает.
        let entry = state
            .store
            .add_subscription("", "https://secret.invalid/sub", "", 12)
            .unwrap();
        subscription_fetch_inner(&state, &entry.id, 1_800_000_000).unwrap();

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
    fn rules_list_marks_missing_paths_without_touching_saved_rules() {
        let state = state_with(Ok(fixture()));
        let live_path = temp_path("rule-live");
        std::fs::write(&live_path, b"").unwrap();
        let live = live_path.to_string_lossy().to_string();
        let gone = temp_path("rule-gone-missing").to_string_lossy().to_string();
        for (id, path) in [("rule-gone", gone.clone()), ("rule-live", live.clone())] {
            state
                .store
                .upsert_rule(AppRule {
                    id: id.to_string(),
                    name: id.to_string(),
                    platform: Platform::Linux,
                    process_name: Vec::new(),
                    process_path: vec![path],
                    path_regex: Vec::new(),
                    enabled: true,
                })
                .unwrap();
        }
        let regex_rule = AppRule {
            id: "rule-regex".to_string(),
            name: "regex".to_string(),
            platform: Platform::Macos,
            process_name: Vec::new(),
            process_path: Vec::new(),
            path_regex: vec!["^/Applications/.*\\.app/".to_string()],
            enabled: true,
        };
        state.store.upsert_rule(regex_rule.clone()).unwrap();

        let views = rules_list_inner(&state);
        let by_id = |id: &str| views.iter().find(|view| view.rule.id == id).unwrap();
        assert_eq!(by_id("rule-gone").missing_paths, vec![gone]);
        assert!(by_id("rule-live").missing_paths.is_empty());
        // Regex с метасимволами не проверяется: ложного «неактивно» нет.
        assert!(by_id("rule-regex").missing_paths.is_empty());
        // Сохранённые данные — по-прежнему только `AppRule`.
        assert_eq!(
            state
                .store
                .rules()
                .into_iter()
                .find(|r| r.id == "rule-regex"),
            Some(regex_rule)
        );
        let _ = std::fs::remove_file(&live_path);
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
    fn apps_running_returns_sorted_limited_list_and_logs_summary() {
        let state = state_with(Ok(fixture()));
        let apps = apps_running_inner(&state);
        assert!(apps.len() <= catalog::running::MAX_RUNNING);
        assert!(apps
            .iter()
            .all(|app| app.pid > 0 && !app.path.trim().is_empty()));
        let names: Vec<String> = apps.iter().map(|app| app.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "список отсортирован по имени");
        assert!(state
            .logs
            .list(None, 100)
            .iter()
            .any(|entry| entry.message.contains("Запущенные процессы: найдено")));
    }

    #[test]
    fn app_rule_from_running_is_disabled_and_uses_bundle_regex() {
        let rule = app_rule_from_running(RunningApp {
            pid: 42,
            name: "Google Chrome".to_string(),
            path: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".to_string(),
            platform: Platform::Macos,
        });
        assert!(!rule.enabled);
        assert_eq!(
            rule.path_regex,
            vec![r"^/Applications/Google Chrome\.app/".to_string()]
        );
        assert!(rule.process_path.is_empty());
    }

    #[test]
    fn logs_capture_events() {
        let state = state_with(Ok(fixture()));
        fetch_sub(&state).unwrap();
        let entries = state.logs.list(None, 10);
        assert!(!entries.is_empty());
    }

    #[test]
    fn connect_logs_proxy_dns_mode_without_warning() {
        let state = state_with(Ok(fixture()));
        fetch_sub(&state).unwrap();
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
        fetch_sub(&state).unwrap();
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
    fn connect_passes_kill_switch_setting_to_helper() {
        let helper = Arc::new(MockHelperClient::new());
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("kill-switch")),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        );
        fetch_sub(&state).unwrap();

        let mut settings = state.store.settings();
        settings.network.kill_switch = true;
        settings_update_inner(&state, settings).unwrap();

        let status = connect_inner(&state).unwrap();
        assert!(helper.last_kill_switch());
        assert!(status.kill_switch.enabled);
        assert!(
            state
                .logs
                .list(Some(LogLevel::Info), 20)
                .iter()
                .any(|entry| entry.message.contains("Kill switch")),
            "включение kill switch попадает в лог"
        );

        let stopped = disconnect_inner(&state).unwrap();
        assert!(!stopped.kill_switch.enabled, "после stop правила сняты");
    }

    /// Предсказуемые задержки по адресу профиля из фикстуры.
    struct FixedProbe;

    impl LatencyProbe for FixedProbe {
        fn probe(
            &self,
            profile: &Profile,
            _timeout: std::time::Duration,
        ) -> Result<u16, latency::PingErrorCode> {
            match profile.server.as_str() {
                "203.0.113.10" => Ok(40),
                "203.0.113.20" => Ok(120),
                _ => Err(latency::PingErrorCode::Unreachable),
            }
        }
    }

    /// Текущая локация медленная, альтернатива быстрая.
    struct DegradedProbe;

    impl LatencyProbe for DegradedProbe {
        fn probe(
            &self,
            profile: &Profile,
            _timeout: std::time::Duration,
        ) -> Result<u16, latency::PingErrorCode> {
            match profile.server.as_str() {
                "203.0.113.10" => Ok(300),
                "203.0.113.20" => Ok(50),
                _ => Err(latency::PingErrorCode::Unreachable),
            }
        }
    }

    #[test]
    fn profiles_ping_reports_latencies_and_hides_addresses() {
        let state = state_with(Ok(fixture())).with_test_probe(Arc::new(FixedProbe));
        fetch_sub(&state).unwrap();

        let response = profiles_ping_inner(&state);
        assert_eq!(response.results.len(), 2);
        assert_eq!(response.results[0].latency_ms, Some(40));
        assert_eq!(response.results[1].latency_ms, Some(120));
        assert_eq!(
            response.best_profile_id.as_deref(),
            Some(response.results[0].profile_id.as_str())
        );
        assert!(response.switch_to.is_none(), "автопереключение выключено");

        let logs = state.logs.list(None, 50);
        assert!(logs.iter().any(|entry| entry.message.contains("Задержка")));
        assert!(
            logs.iter()
                .all(|entry| !entry.message.contains("203.0.113")),
            "адрес сервера не должен попадать в лог"
        );
    }

    #[test]
    fn auto_switch_waits_for_confirmations_and_reconnects() {
        let helper = Arc::new(MockHelperClient::new());
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("auto-switch")),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        )
        .with_test_probe(Arc::new(DegradedProbe));
        fetch_sub(&state).unwrap();
        let mut settings = state.store.settings();
        settings.preferences.auto_select.auto_switch = true;
        settings.preferences.auto_select.confirmations = 2;
        settings_update_inner(&state, settings).unwrap();
        connect_inner(&state).unwrap();

        assert!(profiles_ping_inner(&state).switch_to.is_none());
        let second = profiles_ping_inner(&state);
        let target = second
            .switch_to
            .clone()
            .expect("переключение после двух проверок");
        assert_eq!(second.best_profile_id.as_deref(), Some(target.as_str()));

        let status = switch_profile_inner(&state, &target).unwrap();
        assert_eq!(status.state, CoreState::Running);
        assert_eq!(helper.stop_calls(), 1);
        assert_eq!(helper.start_calls(), 2);
        assert_eq!(
            helper.last_config().unwrap()["outbounds"][0]["server"],
            "203.0.113.20"
        );
        assert_eq!(
            state.store.data().selected_profile_id.as_deref(),
            Some(target.as_str())
        );
        assert!(state
            .logs
            .list(None, 50)
            .iter()
            .any(|entry| entry.message.contains("Переключение локации")));
    }

    #[test]
    fn switch_without_tunnel_only_saves_selection() {
        let helper = Arc::new(MockHelperClient::new());
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("switch-idle")),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        );
        fetch_sub(&state).unwrap();
        let second = state.store.data().all_profiles()[1].id.clone();

        let status = switch_profile_inner(&state, &second).unwrap();
        assert_eq!(status.state, CoreState::Stopped);
        assert_eq!(helper.start_calls(), 0);
        assert_eq!(
            state.store.data().selected_profile_id.as_deref(),
            Some(second.as_str())
        );
        assert_eq!(
            switch_profile_inner(&state, "missing").unwrap_err().code,
            "PROFILE_MISSING"
        );
    }

    /// Второй `start` (новая локация) падает, восстановление прежней проходит.
    struct FailSecondStartHelper {
        inner: MockHelperClient,
        starts: std::sync::atomic::AtomicU32,
    }

    impl FailSecondStartHelper {
        fn new() -> Self {
            Self {
                inner: MockHelperClient::new(),
                starts: std::sync::atomic::AtomicU32::new(0),
            }
        }
    }

    impl HelperClient for FailSecondStartHelper {
        fn status(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.status()
        }

        fn start(
            &self,
            config: serde_json::Value,
            kill_switch: bool,
        ) -> Result<HelperStatus, HelperClientError> {
            if self
                .starts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                == 1
            {
                return Err(HelperClientError::Helper {
                    code: ErrorCode::StartFailed,
                    message: "синтетическая ошибка старта".to_string(),
                });
            }
            self.inner.start(config, kill_switch)
        }

        fn stop(&self) -> Result<HelperStatus, HelperClientError> {
            self.inner.stop()
        }
    }

    #[test]
    fn failed_switch_restores_previous_location_and_kill_switch() {
        let helper = Arc::new(FailSecondStartHelper::new());
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("switch-restore")),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        );
        fetch_sub(&state).unwrap();
        let mut settings = state.store.settings();
        settings.network.kill_switch = true;
        settings_update_inner(&state, settings).unwrap();
        let first = state.store.data().selected_profile_id.clone().unwrap();
        let second = state.store.data().all_profiles()[1].id.clone();
        connect_inner(&state).unwrap();

        let error = switch_profile_inner(&state, &second).unwrap_err();
        assert_eq!(error.code, "HELPER_ERROR");
        assert_eq!(
            state.store.data().selected_profile_id.as_deref(),
            Some(first.as_str()),
            "после неудачного переключения выбран прежний профиль"
        );
        assert_eq!(
            helper.status().unwrap().state,
            CoreState::Running,
            "прежняя локация восстановлена, сеть не осталась без туннеля"
        );
        assert!(
            helper.inner.last_kill_switch(),
            "kill switch не снимается при переключении"
        );
        assert!(state
            .logs
            .list(Some(LogLevel::Warn), 20)
            .iter()
            .any(|entry| entry.message.contains("восстановлена прежняя локация")));
    }

    #[test]
    fn kill_switch_helper_error_maps_to_own_code() {
        let error = CommandError::from(HelperClientError::Helper {
            code: ErrorCode::KillSwitchFailed,
            message: "нужны права root".to_string(),
        });
        assert_eq!(error.code, "KILL_SWITCH_FAILED");
        assert!(error.message.contains("нужны права root"));
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

    /// Ошибки обновлений приходят в UI стабильными кодами, а не одной строкой.
    #[test]
    fn update_errors_map_to_distinct_codes() {
        let cases = [
            (UpdateError::NotConfigured, "UPDATE_NOT_CONFIGURED"),
            (UpdateError::Config, "UPDATE_CONFIG"),
            (UpdateError::CheckFailed, "UPDATE_CHECK_FAILED"),
            (UpdateError::Signature, "UPDATE_SIGNATURE"),
            (UpdateError::DownloadFailed, "UPDATE_DOWNLOAD_FAILED"),
            (UpdateError::InstallFailed, "UPDATE_INSTALL_FAILED"),
            (UpdateError::NotAvailable, "UPDATE_NOT_AVAILABLE"),
        ];
        for (error, code) in cases {
            let command_error = CommandError::from(error);
            assert_eq!(command_error.code, code);
            assert!(!command_error.message.is_empty());
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

        let error = fetch_sub(&state).unwrap_err();
        assert_eq!(error.code, "UNAUTHORIZED");
        assert!(state.store.data().all_profiles().is_empty());

        let response = fetch_sub(&state).unwrap();
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
        assert!(empty.subscriptions.is_empty());
        assert!(empty.selected_profile_id.is_none());
        assert!(empty.rules.is_empty());

        // Успешное обновление: два профиля, первый выбран автоматически.
        let fetched = fetch_sub(&state).unwrap();
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

    /// Connect включает loopback Clash API; stop сбрасывает сессию статистики.
    #[test]
    fn connect_exposes_loopback_stats_and_stop_resets_them() {
        let helper = Arc::new(MockHelperClient::new());
        let state = AppState::new(
            helper.clone(),
            Arc::new(UnsupportedInstaller),
            Store::load(temp_path("traffic-lifecycle")),
            Arc::new(MockFetcher {
                body: Ok(fixture()),
            }),
        );
        fetch_sub(&state).unwrap();

        connect_inner(&state).unwrap();
        let first = state.traffic_endpoint().expect("сессия статистики создана");
        let config = helper.last_config().expect("конфиг передан helper'у");
        assert_eq!(
            config["experimental"]["clash_api"]["external_controller"],
            format!("127.0.0.1:{}", first.api().port)
        );
        assert_eq!(
            config["experimental"]["clash_api"]["secret"],
            first.api().secret
        );

        disconnect_inner(&state).unwrap();
        assert!(
            state.traffic_endpoint().is_none(),
            "после stop источник статистики недоступен"
        );
        assert_eq!(traffic_stats_inner(&state), TrafficStats::unavailable());

        // Новый start — новая сессия: другой secret, счётчики с нуля.
        connect_inner(&state).unwrap();
        let second = state.traffic_endpoint().expect("новая сессия");
        assert_ne!(first.api().secret, second.api().secret);
    }

    #[test]
    fn traffic_stats_are_unavailable_without_session() {
        let state = state_with(Ok(fixture()));
        assert_eq!(traffic_stats_inner(&state), TrafficStats::unavailable());
        assert!(
            state
                .logs
                .list(Some(LogLevel::Warn), 10)
                .iter()
                .all(|entry| !entry.message.contains("Статистика трафика")),
            "без сессии предупреждений быть не должно"
        );
    }

    #[test]
    fn traffic_stats_return_totals_from_local_api() {
        crate::init_tls();
        let body = r#"{"connections":[],"downloadTotal":8192,"memory":1,"uploadTotal":2048}"#;
        let (port, handle) = spawn_traffic_api(body);
        let state = state_with(Ok(fixture()));
        state.set_traffic_endpoint(Some(TrafficEndpoint::with_parts(port, "test-secret")));

        assert_eq!(
            traffic_stats_inner(&state),
            TrafficStats {
                available: true,
                upload_total: 2048,
                download_total: 8192,
            }
        );
        handle.join().unwrap();
    }

    #[test]
    fn traffic_stats_error_is_unavailable_and_warned_once() {
        crate::init_tls();
        // Порт закрыт: опрос не удаётся, но подключение не затрагивается.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let state = state_with(Ok(fixture()));
        state.set_traffic_endpoint(Some(TrafficEndpoint::with_parts(port, "test-secret")));

        assert_eq!(traffic_stats_inner(&state), TrafficStats::unavailable());
        assert_eq!(traffic_stats_inner(&state), TrafficStats::unavailable());

        let warnings: Vec<_> = state
            .logs
            .list(Some(LogLevel::Warn), 20)
            .into_iter()
            .filter(|entry| entry.message.contains("Статистика трафика недоступна"))
            .collect();
        assert_eq!(warnings.len(), 1, "серия ошибок пишется в лог один раз");
    }
}
