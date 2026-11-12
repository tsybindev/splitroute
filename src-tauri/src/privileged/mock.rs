//! Mock-клиент helper'а для тестов и разработки без установленного helper'а.

use std::sync::Mutex;

use serde_json::Value;
use splitroute_protocol::{CoreState, HelperStatus};

use super::{HelperClient, HelperClientError};

#[derive(Debug, Default)]
struct MockState {
    running: bool,
    started_at: Option<u64>,
    start_calls: u32,
    stop_calls: u32,
}

/// In-memory реализация `HelperClient`: идемпотентные start/stop, предсказуемый статус.
#[derive(Debug, Default)]
pub struct MockHelperClient {
    state: Mutex<MockState>,
}

impl MockHelperClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Сколько раз вызывали `start` (для тестов).
    pub fn start_calls(&self) -> u32 {
        self.state.lock().unwrap().start_calls
    }

    /// Сколько раз вызывали `stop` (для тестов).
    pub fn stop_calls(&self) -> u32 {
        self.state.lock().unwrap().stop_calls
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
        }
    }
}

impl HelperClient for MockHelperClient {
    fn status(&self) -> Result<HelperStatus, HelperClientError> {
        let state = self.state.lock().unwrap();
        Ok(Self::status_of(&state))
    }

    fn start(&self, _config: Value) -> Result<HelperStatus, HelperClientError> {
        let mut state = self.state.lock().unwrap();
        state.start_calls += 1;
        if !state.running {
            state.running = true;
            state.started_at = Some(1_762_950_000);
        }
        Ok(Self::status_of(&state))
    }

    fn stop(&self) -> Result<HelperStatus, HelperClientError> {
        let mut state = self.state.lock().unwrap();
        state.stop_calls += 1;
        state.running = false;
        state.started_at = None;
        Ok(Self::status_of(&state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_is_idempotent() {
        let client = MockHelperClient::new();
        let first = client.start(Value::Null).unwrap();
        let second = client.start(Value::Null).unwrap();
        assert_eq!(first.state, CoreState::Running);
        assert_eq!(second.state, CoreState::Running);
        assert_eq!(client.start_calls(), 2);
    }

    #[test]
    fn stop_is_idempotent() {
        let client = MockHelperClient::new();
        client.start(Value::Null).unwrap();
        assert_eq!(client.stop().unwrap().state, CoreState::Stopped);
        assert_eq!(client.stop().unwrap().state, CoreState::Stopped);
        assert_eq!(client.stop_calls(), 2);
    }

    #[test]
    fn status_reflects_state() {
        let client = MockHelperClient::new();
        assert_eq!(client.status().unwrap().state, CoreState::Stopped);
        client.start(Value::Null).unwrap();
        let status = client.status().unwrap();
        assert_eq!(status.state, CoreState::Running);
        assert_eq!(status.pid, Some(4242));
    }
}
