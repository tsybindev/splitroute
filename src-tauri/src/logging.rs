//! Кольцевой буфер логов приложения для экрана «Логи».
//!
//! Сюда попадают только сообщения, сформированные кодом SplitRoute. URL
//! подписки, HWID и UUID в буфер не пишутся.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;
use splitroute_protocol::LogLevel;

use crate::store::now_secs;

/// Одна строка лога.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogEntry {
    /// Unix-время, секунды.
    pub timestamp: u64,
    pub level: LogLevel,
    pub message: String,
}

/// Ограниченный по размеру буфер последних строк.
pub struct LogBuffer {
    entries: Mutex<VecDeque<LogEntry>>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(capacity.min(1024))),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&self, level: LogLevel, message: impl Into<String>) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() == self.capacity {
            entries.pop_front();
        }
        entries.push_back(LogEntry {
            timestamp: now_secs(),
            level,
            message: message.into(),
        });
    }

    /// Последние строки, опционально отфильтрованные по минимальному уровню.
    pub fn list(&self, min_level: Option<LogLevel>, limit: usize) -> Vec<LogEntry> {
        let entries = self.entries.lock().unwrap();
        let mut filtered: Vec<LogEntry> = entries
            .iter()
            .filter(|entry| min_level.is_none_or(|min| level_rank(entry.level) >= level_rank(min)))
            .cloned()
            .collect();
        if filtered.len() > limit {
            filtered.drain(..filtered.len() - limit);
        }
        filtered
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Порядок уровней: больше — важнее.
pub fn level_rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Trace => 0,
        LogLevel::Debug => 1,
        LogLevel::Info => 2,
        LogLevel::Warn => 3,
        LogLevel::Error => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_last_entries() {
        let buffer = LogBuffer::new(3);
        for index in 0..5 {
            buffer.push(LogLevel::Info, format!("строка {index}"));
        }
        let entries = buffer.list(None, 10);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "строка 2");
        assert_eq!(entries[2].message, "строка 4");
    }

    #[test]
    fn filters_by_level() {
        let buffer = LogBuffer::new(10);
        buffer.push(LogLevel::Debug, "debug");
        buffer.push(LogLevel::Info, "info");
        buffer.push(LogLevel::Error, "error");
        buffer.push(LogLevel::Warn, "warn");
        let entries = buffer.list(Some(LogLevel::Warn), 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "error");
        assert_eq!(entries[1].message, "warn");
    }

    #[test]
    fn respects_limit() {
        let buffer = LogBuffer::new(10);
        for index in 0..5 {
            buffer.push(LogLevel::Info, format!("{index}"));
        }
        assert_eq!(buffer.list(None, 2).len(), 2);
    }
}
