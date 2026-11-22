//! Mock-клиент helper'а для тестов и разработки без установленного helper'а.

use std::sync::Mutex;

use serde_json::Value;
use splitroute_protocol::{CoreState, ErrorCode, HelperStatus, KillSwitchStatus};

use super::{HelperClient, HelperClientError};

#[derive(Debug, Default)]
struct MockState {
    running: bool,
    started_at: Option<u64>,
    start_calls: u32,
    reload_calls: u32,
    restart_calls: u32,
    stop_calls: u32,
    last_config: Option<Value>,
    last_rule_set: Option<Value>,
    /// Правила kill switch действуют, пока идёт сессия.
    kill_switch: bool,
}

/// In-memory реализация `HelperClient`: идемпотентные start/stop, предсказуемый статус.
#[derive(Debug, Default)]
pub struct MockHelperClient {
    state: Mutex<MockState>,
    /// Неудача, которую вернёт следующий `restart` (инжекция для тестов отката).
    restart_failure: Mutex<Option<HelperClientError>>,
}

impl MockHelperClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Сколько раз вызывали `start` (для тестов).
    pub fn start_calls(&self) -> u32 {
        self.state.lock().unwrap().start_calls
    }

    /// Сколько раз вызывали `restart` (для тестов).
    pub fn restart_calls(&self) -> u32 {
        self.state.lock().unwrap().restart_calls
    }

    /// Назначает ошибку следующему вызову `restart` (для тестов).
    pub fn set_restart_failure(&self, failure: Option<HelperClientError>) {
        *self.restart_failure.lock().unwrap() = failure;
    }

    /// Сколько раз вызывали `stop` (для тестов).
    pub fn stop_calls(&self) -> u32 {
        self.state.lock().unwrap().stop_calls
    }

    /// Конфиг, переданный в последний `start` (для тестов).
    pub fn last_config(&self) -> Option<Value> {
        self.state.lock().unwrap().last_config.clone()
    }

    /// Запрошенный в последнем `start` режим kill switch (для тестов).
    pub fn last_kill_switch(&self) -> bool {
        self.state.lock().unwrap().kill_switch
    }

    /// Сколько раз вызывали `reload_routing` (для тестов).
    pub fn reload_calls(&self) -> u32 {
        self.state.lock().unwrap().reload_calls
    }

    /// Rule-set, переданный в последний `reload_routing` (для тестов).
    pub fn last_rule_set(&self) -> Option<Value> {
        self.state.lock().unwrap().last_rule_set.clone()
    }

    fn status_of(state: &MockState) -> HelperStatus {
        HelperStatus {
            state: if state.running {
                CoreState::Running
            } else {
                CoreState::Stopped
            },
            pid: state.running.then_some(4242),
            version: Some("mock sing-box 1.14.4".to_string()),
            started_at: state.started_at,
            kill_switch: KillSwitchStatus {
                enabled: state.running && state.kill_switch,
                error: None,
            },
        }
    }
}

impl HelperClient for MockHelperClient {
    fn status(&self) -> Result<HelperStatus, HelperClientError> {
        let state = self.state.lock().unwrap();
        Ok(Self::status_of(&state))
    }

    fn start(
        &self,
        config: Value,
        rule_set: Value,
        kill_switch: bool,
    ) -> Result<HelperStatus, HelperClientError> {
        let mut state = self.state.lock().unwrap();
        state.start_calls += 1;
        state.last_config = Some(config);
        state.last_rule_set = Some(rule_set);
        state.kill_switch = kill_switch;
        if !state.running {
            state.running = true;
            state.started_at = Some(crate::store::now_secs());
        }
        Ok(Self::status_of(&state))
    }

    fn reload_routing(&self, rule_set: Value) -> Result<HelperStatus, HelperClientError> {
        let mut state = self.state.lock().unwrap();
        if !state.running {
            return Err(HelperClientError::Helper {
                code: ErrorCode::ReloadFailed,
                message: "нет активной сессии".to_string(),
            });
        }
        state.reload_calls += 1;
        state.last_rule_set = Some(rule_set);
        Ok(Self::status_of(&state))
    }

    fn restart(
        &self,
        config: Value,
        rule_set: Value,
        kill_switch: bool,
    ) -> Result<HelperStatus, HelperClientError> {
        let mut state = self.state.lock().unwrap();
        // Счётчик считает попытки, включая инжектированную неудачу.
        state.restart_calls += 1;
        // Инжектированная неудача имитирует helper: рестарт упал — туннель
        // остановлен, правила сняты (S12-T1).
        if let Some(error) = self.restart_failure.lock().unwrap().take() {
            state.running = false;
            state.started_at = None;
            state.kill_switch = false;
            return Err(error);
        }
        state.last_config = Some(config);
        state.last_rule_set = Some(rule_set);
        state.kill_switch = kill_switch;
        // Рестарт живой сессии сохраняет состояние «Подключено», но ядро —
        // новый процесс: сессия статистики начинается заново.
        state.started_at = Some(crate::store::now_secs());
        state.running = true;
        Ok(Self::status_of(&state))
    }

    fn stop(&self) -> Result<HelperStatus, HelperClientError> {
        let mut state = self.state.lock().unwrap();
        state.stop_calls += 1;
        state.running = false;
        state.started_at = None;
        state.kill_switch = false;
        Ok(Self::status_of(&state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_is_idempotent() {
        let client = MockHelperClient::new();
        let first = client.start(Value::Null, Value::Null, false).unwrap();
        let second = client.start(Value::Null, Value::Null, false).unwrap();
        assert_eq!(first.state, CoreState::Running);
        assert_eq!(second.state, CoreState::Running);
        assert_eq!(client.start_calls(), 2);
    }

    #[test]
    fn stop_is_idempotent() {
        let client = MockHelperClient::new();
        client.start(Value::Null, Value::Null, false).unwrap();
        assert_eq!(client.stop().unwrap().state, CoreState::Stopped);
        assert_eq!(client.stop().unwrap().state, CoreState::Stopped);
        assert_eq!(client.stop_calls(), 2);
    }

    #[test]
    fn status_reflects_state() {
        let client = MockHelperClient::new();
        assert_eq!(client.status().unwrap().state, CoreState::Stopped);
        client.start(Value::Null, Value::Null, false).unwrap();
        let status = client.status().unwrap();
        assert_eq!(status.state, CoreState::Running);
        assert_eq!(status.pid, Some(4242));
    }

    #[test]
    fn start_records_config_and_current_time() {
        let client = MockHelperClient::new();
        let before = crate::store::now_secs();
        let config = serde_json::json!({ "log": { "level": "info" } });
        let status = client.start(config.clone(), Value::Null, false).unwrap();
        let after = crate::store::now_secs();

        let started_at = status.started_at.expect("время старта");
        assert!((before..=after).contains(&started_at));
        assert_eq!(client.last_config(), Some(config));
        assert_eq!(client.stop().unwrap().started_at, None);
    }

    #[test]
    fn kill_switch_state_follows_session() {
        let client = MockHelperClient::new();
        assert!(!client.status().unwrap().kill_switch.enabled);

        let status = client.start(Value::Null, Value::Null, true).unwrap();
        assert!(client.last_kill_switch());
        assert!(status.kill_switch.enabled);

        let stopped = client.stop().unwrap();
        assert!(!stopped.kill_switch.enabled);
    }

    #[test]
    fn reload_routing_requires_running_session() {
        let client = MockHelperClient::new();
        assert!(client.reload_routing(Value::Null).is_err());

        client.start(Value::Null, Value::Null, false).unwrap();
        let rule_set = serde_json::json!({ "version": 2, "rules": [] });
        client.reload_routing(rule_set.clone()).unwrap();
        assert_eq!(client.reload_calls(), 1);
        assert_eq!(client.last_rule_set(), Some(rule_set));

        client.stop().unwrap();
        assert!(client.reload_routing(Value::Null).is_err());
    }

    #[test]
    fn restart_keeps_running_session_without_stop() {
        let client = MockHelperClient::new();
        client.start(Value::Null, Value::Null, true).unwrap();

        let status = client
            .restart(serde_json::json!({ "log": {} }), Value::Null, true)
            .unwrap();
        assert_eq!(status.state, CoreState::Running);
        assert!(status.kill_switch.enabled);
        assert_eq!(client.restart_calls(), 1);
        assert_eq!(client.stop_calls(), 0, "restart не останавливает сессию");
    }

    #[test]
    fn injected_restart_failure_stops_session() {
        // Имитация helper'а: неудачный рестарт останавливает туннель и снимает
        // правила (S12-T1) — на этот конец в приложении есть откат.
        let client = MockHelperClient::new();
        client.start(Value::Null, Value::Null, true).unwrap();
        client.set_restart_failure(Some(HelperClientError::Helper {
            code: ErrorCode::StartFailed,
            message: "не удалось запустить ядро".to_string(),
        }));

        let error = client.restart(Value::Null, Value::Null, true).unwrap_err();
        assert!(matches!(
            error,
            HelperClientError::Helper {
                code: ErrorCode::StartFailed,
                ..
            }
        ));
        assert_eq!(client.status().unwrap().state, CoreState::Stopped);
        assert!(!client.status().unwrap().kill_switch.enabled);
    }
}
