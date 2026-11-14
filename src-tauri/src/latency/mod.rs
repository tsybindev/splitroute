//! Замер задержки локаций и выбор лучшей (PRD §6.1, §6.6).
//!
//! Задержка — это TCP-connect до `server:port`: ядро не запускается, helper не
//! участвует, UUID и TLS-параметры не используются. Замеряется только время
//! установления TCP-соединения, а не скорость прокси.

use std::collections::HashMap;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::Serialize;
use splitroute_protocol::Profile;

/// Таймаут одного замера: медленная локация не должна задерживать остальные.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Больше профилей не проверяем: список локаций у провайдеров небольшой.
pub const MAX_PROFILES: usize = 64;
/// Одновременных замеров: ограничивает число потоков и нагрузку на сеть.
pub const MAX_CONCURRENCY: usize = 8;

/// Типизированная причина неудачного замера.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PingErrorCode {
    #[serde(rename = "PING_TIMEOUT")]
    Timeout,
    #[serde(rename = "PING_UNREACHABLE")]
    Unreachable,
    #[serde(rename = "PING_DNS")]
    Dns,
}

impl PingErrorCode {
    /// Код для логов и UI; адрес сервера в лог не попадает.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "PING_TIMEOUT",
            Self::Unreachable => "PING_UNREACHABLE",
            Self::Dns => "PING_DNS",
        }
    }
}

/// Результат замера одной локации.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PingResult {
    pub profile_id: String,
    pub latency_ms: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PingErrorCode>,
}

impl PingResult {
    fn ok(profile_id: &str, latency_ms: u16) -> Self {
        Self {
            profile_id: profile_id.to_string(),
            latency_ms: Some(latency_ms),
            error: None,
        }
    }

    fn failed(profile_id: &str, error: PingErrorCode) -> Self {
        Self {
            profile_id: profile_id.to_string(),
            latency_ms: None,
            error: Some(error),
        }
    }
}

/// Источник замеров: настоящий TCP или предсказуемая заглушка в тестах.
pub trait LatencyProbe: Send + Sync {
    fn probe(&self, profile: &Profile, timeout: Duration) -> Result<u16, PingErrorCode>;
}

/// TCP-connect до сервера профиля.
pub struct TcpProbe;

impl LatencyProbe for TcpProbe {
    fn probe(&self, profile: &Profile, timeout: Duration) -> Result<u16, PingErrorCode> {
        let addresses = (profile.server.as_str(), profile.port)
            .to_socket_addrs()
            .map_err(|_| PingErrorCode::Dns)?;

        let started = Instant::now();
        let mut resolved = false;
        let mut last_error = PingErrorCode::Unreachable;
        for address in addresses {
            resolved = true;
            match TcpStream::connect_timeout(&address, timeout) {
                // Соединение сразу закрывается: замеряется только рукопожатие.
                Ok(stream) => {
                    drop(stream);
                    return Ok(millis_since(started));
                }
                Err(error) => {
                    last_error = if error.kind() == io::ErrorKind::TimedOut {
                        PingErrorCode::Timeout
                    } else {
                        PingErrorCode::Unreachable
                    };
                }
            }
        }

        Err(if resolved {
            last_error
        } else {
            PingErrorCode::Dns
        })
    }
}

fn millis_since(started: Instant) -> u16 {
    let elapsed = started.elapsed().as_millis();
    elapsed.clamp(1, u16::MAX as u128) as u16
}

/// Замеряет все локации параллельно, сохраняя порядок профилей в ответе.
///
/// Список обрезается до [`MAX_PROFILES`], одновременно работают не больше
/// [`MAX_CONCURRENCY`] потоков. Паника одного замера не роняет остальные.
pub fn measure(
    probe: &dyn LatencyProbe,
    profiles: &[Profile],
    timeout: Duration,
) -> Vec<PingResult> {
    let profiles = &profiles[..profiles.len().min(MAX_PROFILES)];
    let mut results = Vec::with_capacity(profiles.len());

    for chunk in profiles.chunks(MAX_CONCURRENCY) {
        let part: Vec<PingResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|profile| (profile, scope.spawn(move || probe.probe(profile, timeout))))
                .collect();
            handles
                .into_iter()
                .map(|(profile, handle)| match handle.join() {
                    Ok(Ok(latency)) => PingResult::ok(&profile.id, latency),
                    Ok(Err(error)) => PingResult::failed(&profile.id, error),
                    Err(_) => PingResult::failed(&profile.id, PingErrorCode::Unreachable),
                })
                .collect()
        });
        results.extend(part);
    }

    results
}

/// Лучшая локация: минимальная задержка, при равенстве — первая в подписке.
pub fn best_profile(results: &[PingResult]) -> Option<&PingResult> {
    results
        .iter()
        .filter(|result| result.latency_ms.is_some())
        .min_by_key(|result| result.latency_ms.unwrap_or(u16::MAX))
}

/// Правила автопереключения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchPolicy {
    /// Насколько новая локация должна быть быстрее текущей, мс.
    pub degradation_ms: u16,
    /// Сколько проверок подряд должна подтверждаться деградация.
    pub confirmations: u8,
    /// Сколько неудачных замеров подряд достаточно для переключения.
    pub failures_to_switch: u8,
}

/// Память между проверками: серии неудач и подтверждённая деградация.
///
/// Живёт в памяти приложения: после перезапуска счётчики начинаются заново,
/// что безопаснее мгновенного переключения по одному замеру.
#[derive(Debug, Default)]
pub struct LatencyTracker {
    failures: HashMap<String, u8>,
    degradation: Option<(String, u8)>,
}

impl LatencyTracker {
    /// Сбрасывает накопленные серии (после ручного выбора или переключения).
    pub fn reset(&mut self) {
        self.failures.clear();
        self.degradation = None;
    }

    /// Обновляет состояние и возвращает локацию, на которую нужно переключиться.
    ///
    /// Возвращает `None`, если текущая локация не задана, альтернатив нет, порог
    /// не превышен или подтверждений ещё недостаточно.
    pub fn observe(
        &mut self,
        current: Option<&str>,
        results: &[PingResult],
        policy: SwitchPolicy,
    ) -> Option<String> {
        let current = current?;

        for result in results {
            let streak = self.failures.entry(result.profile_id.clone()).or_insert(0);
            if result.error.is_some() {
                *streak = streak.saturating_add(1);
            } else {
                *streak = 0;
            }
        }

        let best_other = results
            .iter()
            .filter(|result| result.profile_id != current && result.latency_ms.is_some())
            .min_by_key(|result| result.latency_ms.unwrap_or(u16::MAX));
        let Some(best_other) = best_other else {
            self.degradation = None;
            return None;
        };

        let current_result = results
            .iter()
            .find(|result| result.profile_id == current)
            .filter(|result| result.latency_ms.is_some());
        let Some(current_result) = current_result else {
            // Текущая локация не отвечает: переключаем только после серии неудач.
            let failures = self.failures.get(current).copied().unwrap_or(0);
            self.degradation = None;
            return (failures >= policy.failures_to_switch.max(1))
                .then(|| best_other.profile_id.clone());
        };

        let current_ms = current_result.latency_ms.unwrap_or(u16::MAX);
        let best_ms = best_other.latency_ms.unwrap_or(u16::MAX);
        let improvement = current_ms.saturating_sub(best_ms);
        // Гистерезис: улучшение должно быть заметным, иначе не дёргаем туннель.
        let degraded = improvement > 0 && improvement >= policy.degradation_ms;
        if !degraded {
            self.degradation = None;
            return None;
        }

        let streak = match &self.degradation {
            Some((id, count)) if id == &best_other.profile_id => count.saturating_add(1),
            _ => 1,
        };
        if streak >= policy.confirmations.max(1) {
            self.degradation = None;
            return Some(best_other.profile_id.clone());
        }
        self.degradation = Some((best_other.profile_id.clone(), streak));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splitroute_protocol::{Protocol, TlsConfig, TlsKind, Transport};
    use std::net::TcpListener;
    use std::sync::Arc;

    fn profile(id: &str, server: &str) -> Profile {
        Profile {
            id: id.to_string(),
            name: format!("Локация {id}"),
            protocol: Protocol::Vless,
            server: server.to_string(),
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

    fn ok(id: &str, latency_ms: u16) -> PingResult {
        PingResult::ok(id, latency_ms)
    }

    fn failed(id: &str) -> PingResult {
        PingResult::failed(id, PingErrorCode::Timeout)
    }

    /// Предсказуемые задержки по id профиля.
    struct FakeProbe(Vec<(&'static str, Result<u16, PingErrorCode>)>);

    impl LatencyProbe for FakeProbe {
        fn probe(&self, profile: &Profile, _timeout: Duration) -> Result<u16, PingErrorCode> {
            self.0
                .iter()
                .find(|(id, _)| *id == profile.id)
                .map(|(_, result)| *result)
                .unwrap_or(Err(PingErrorCode::Unreachable))
        }
    }

    #[test]
    fn best_profile_picks_minimum_and_first_on_tie() {
        let results = [ok("a", 80), ok("b", 40), ok("c", 40), failed("d")];
        assert_eq!(best_profile(&results).unwrap().profile_id, "b");
        assert!(best_profile(&[failed("d")]).is_none());
        assert!(best_profile(&[]).is_none());
    }

    #[test]
    fn measure_keeps_order_and_limits_profiles() {
        let profiles: Vec<Profile> = (0..70)
            .map(|index| profile(&format!("p{index}"), "127.0.0.1"))
            .collect();
        let probe = FakeProbe(Vec::new());
        let results = measure(&probe, &profiles, Duration::from_millis(10));
        assert_eq!(results.len(), MAX_PROFILES);
        assert_eq!(results[0].profile_id, "p0");
        assert_eq!(results[MAX_PROFILES - 1].profile_id, "p63");
    }

    #[test]
    fn measure_maps_probe_errors_to_typed_codes() {
        let profiles = [profile("a", "127.0.0.1"), profile("b", "127.0.0.1")];
        let probe = FakeProbe(vec![("a", Ok(12)), ("b", Err(PingErrorCode::Dns))]);
        let results = measure(&probe, &profiles, Duration::from_millis(10));
        assert_eq!(results[0], ok("a", 12));
        assert_eq!(results[1].error, Some(PingErrorCode::Dns));
        assert_eq!(results[1].latency_ms, None);
    }

    #[test]
    fn tcp_probe_measures_open_loopback_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut profile = profile("local", "127.0.0.1");
        profile.port = port;

        let latency = TcpProbe
            .probe(&profile, Duration::from_millis(500))
            .unwrap();
        assert!(latency >= 1);

        // Подключение принято и закрыто: больше соединений не требуется.
        drop(listener);
    }

    #[test]
    fn tcp_probe_rejects_closed_port_and_bad_dns() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut closed = profile("closed", "127.0.0.1");
        closed.port = port;
        let closed_error = TcpProbe.probe(&closed, Duration::from_millis(200));
        // Windows на закрытый loopback-порт нередко отвечает не RST, а тишиной.
        #[cfg(unix)]
        assert_eq!(closed_error, Err(PingErrorCode::Unreachable));
        #[cfg(not(unix))]
        assert!(matches!(
            closed_error,
            Err(PingErrorCode::Unreachable) | Err(PingErrorCode::Timeout)
        ));

        let invalid = profile("dns", "invalid.invalid.invalid");
        assert_eq!(
            TcpProbe.probe(&invalid, Duration::from_millis(200)),
            Err(PingErrorCode::Dns)
        );
    }

    fn policy() -> SwitchPolicy {
        SwitchPolicy {
            degradation_ms: 100,
            confirmations: 2,
            failures_to_switch: 2,
        }
    }

    #[test]
    fn tracker_ignores_small_improvement() {
        let mut tracker = LatencyTracker::default();
        let results = [ok("current", 120), ok("other", 80)];
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
    }

    #[test]
    fn tracker_requires_confirmations_before_switch() {
        let mut tracker = LatencyTracker::default();
        let results = [ok("current", 300), ok("other", 50)];
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
        assert_eq!(
            tracker.observe(Some("current"), &results, policy()),
            Some("other".to_string())
        );
    }

    #[test]
    fn tracker_resets_streak_when_degradation_disappears() {
        let mut tracker = LatencyTracker::default();
        let degraded = [ok("current", 300), ok("other", 50)];
        let normal = [ok("current", 60), ok("other", 50)];
        assert_eq!(tracker.observe(Some("current"), &degraded, policy()), None);
        assert_eq!(tracker.observe(Some("current"), &normal, policy()), None);
        assert_eq!(tracker.observe(Some("current"), &degraded, policy()), None);
    }

    #[test]
    fn tracker_switches_after_failure_streak() {
        let mut tracker = LatencyTracker::default();
        let results = [failed("current"), ok("other", 70)];
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
        assert_eq!(
            tracker.observe(Some("current"), &results, policy()),
            Some("other".to_string())
        );
    }

    #[test]
    fn tracker_does_not_switch_without_working_alternative() {
        let mut tracker = LatencyTracker::default();
        let results = [failed("current"), failed("other")];
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
        assert_eq!(tracker.observe(None, &results, policy()), None);
    }

    #[test]
    fn tracker_reset_stops_pending_switch() {
        let mut tracker = LatencyTracker::default();
        let results = [failed("current"), ok("other", 70)];
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
        tracker.reset();
        assert_eq!(tracker.observe(Some("current"), &results, policy()), None);
    }

    #[test]
    fn tracker_is_shareable_between_threads() {
        let tracker = Arc::new(std::sync::Mutex::new(LatencyTracker::default()));
        let clone = tracker.clone();
        std::thread::spawn(move || {
            let results = [ok("current", 300), ok("other", 50)];
            let mut guard = clone.lock().unwrap();
            let _ = guard.observe(Some("current"), &results, policy());
        })
        .join()
        .unwrap();
        assert!(tracker.lock().is_ok());
    }

    #[test]
    fn ping_error_codes_are_stable() {
        assert_eq!(PingErrorCode::Timeout.as_str(), "PING_TIMEOUT");
        assert_eq!(PingErrorCode::Unreachable.as_str(), "PING_UNREACHABLE");
        assert_eq!(PingErrorCode::Dns.as_str(), "PING_DNS");
        assert_eq!(
            serde_json::to_value(PingErrorCode::Timeout).unwrap(),
            "PING_TIMEOUT"
        );
    }
}
