//! Kill switch (fail-closed): правила ставятся helper'ом до запуска ядра.
//!
//! Смысл: трафик, который не идёт через ядро, не должен выходить в интернет.
//! Правила разрешают loopback, локальные сети и трафик root (ядро работает под
//! root), остальное блокируют. Если sing-box упадёт, правила остаются — до
//! `stop`, завершения helper'а или выключения настройки. Снимаются они всегда
//! идемпотентно, а при старте helper'а удаляются «залипшие» правила прошлого
//! запуска.
//!
//! Механизмы по ОС:
//! - macOS — pf anchor `com.apple/splitroute.killswitch`;
//! - Linux — отдельная таблица nftables `inet splitroute_killswitch`;
//! - Windows — собственный провайдер и подслой WFP (фильтры разрешают loopback,
//!   локальные сети, TUN и процесс ядра, остальное блокируют).

pub mod rules;

use std::path::Path;

use splitroute_protocol::KillSwitchStatus;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::SystemFirewall;
#[cfg(target_os = "macos")]
pub use macos::SystemFirewall;
#[cfg(target_os = "windows")]
pub use windows::SystemFirewall;

/// Ошибка применения правил: текст уже безопасен для логов и UI.
pub type FirewallError = String;

/// Минимальная поверхность платформенного firewall'а.
///
/// Ни helper, ни клиент не передают сюда адреса, порты или правила: всё
/// фиксировано в [`rules`]. Методы обязаны быть идемпотентными.
pub trait Firewall: Send {
    fn enable(&self) -> Result<(), FirewallError>;
    fn disable(&self) -> Result<(), FirewallError>;
}

/// Состояние правил в памяти helper'а.
pub struct KillSwitch {
    firewall: Box<dyn Firewall>,
    active: bool,
    error: Option<String>,
}

impl KillSwitch {
    pub fn new(firewall: Box<dyn Firewall>) -> Self {
        Self {
            firewall,
            active: false,
            error: None,
        }
    }

    /// Приводит правила к запрошенному состоянию. Повторный вызов — no-op.
    pub fn sync(&mut self, enabled: bool) -> Result<(), FirewallError> {
        self.error = None;
        if enabled == self.active {
            return Ok(());
        }
        let result = if enabled {
            self.firewall.enable()
        } else {
            self.firewall.disable()
        };
        match result {
            Ok(()) => {
                self.active = enabled;
                Ok(())
            }
            Err(error) => {
                // При ошибке снятия считаем, что правила могли остаться.
                if !enabled {
                    self.active = true;
                }
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    /// Снимает правила при остановке/завершении helper'а.
    ///
    /// Ошибка не прерывает остановку ядра, но попадает в `status`, чтобы UI не
    /// показывал «защищено», когда правила снять не удалось.
    pub fn release(&mut self) {
        if !self.active {
            return;
        }
        match self.firewall.disable() {
            Ok(()) => {
                self.active = false;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
            }
        }
    }

    pub fn status(&self) -> KillSwitchStatus {
        KillSwitchStatus {
            enabled: self.active,
            error: self.error.clone(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Удаляет правила, оставшиеся от прошлого запуска helper'а.
///
/// Вызывается после успешного захвата сокета/пайпа: второй экземпляр helper'а
/// не должен снимать защиту работающего.
pub fn cleanup_stale(runtime_dir: &Path) -> Result<(), FirewallError> {
    platform_firewall(runtime_dir, None).disable()
}

/// Платформенный backend для рабочего helper'а.
///
/// `core` нужен Windows: правила WFP разрешают точный AppId процесса ядра.
/// macOS и Linux путь к ядру не используют.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub fn platform_firewall(runtime_dir: &Path, core: Option<&Path>) -> SystemFirewall {
    SystemFirewall::new(runtime_dir, core)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn platform_firewall(_runtime_dir: &Path, _core: Option<&Path>) -> UnsupportedFirewall {
    UnsupportedFirewall
}

/// Заглушка для ОС, где kill switch не реализован.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub struct UnsupportedFirewall;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
impl Firewall for UnsupportedFirewall {
    fn enable(&self) -> Result<(), FirewallError> {
        Err("kill switch не поддерживается на этой ОС".to_string())
    }

    fn disable(&self) -> Result<(), FirewallError> {
        Ok(())
    }
}

impl KillSwitch {
    /// Backend текущей ОС с доступом к runtime-каталогу и пути к ядру.
    pub fn platform(runtime_dir: &Path, core: &Path) -> Self {
        Self::new(Box::new(platform_firewall(runtime_dir, Some(core))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug, Default)]
    struct Calls {
        enable: AtomicUsize,
        disable: AtomicUsize,
        fail_enable: AtomicUsize,
        fail_disable: AtomicUsize,
    }

    struct FakeFirewall {
        calls: Arc<Calls>,
    }

    impl Firewall for FakeFirewall {
        fn enable(&self) -> Result<(), FirewallError> {
            if self.calls.fail_enable.load(Ordering::SeqCst) > 0 {
                self.calls.fail_enable.fetch_sub(1, Ordering::SeqCst);
                return Err("нет прав".to_string());
            }
            self.calls.enable.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn disable(&self) -> Result<(), FirewallError> {
            if self.calls.fail_disable.load(Ordering::SeqCst) > 0 {
                self.calls.fail_disable.fetch_sub(1, Ordering::SeqCst);
                return Err("не удалось снять правила".to_string());
            }
            self.calls.disable.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn kill_switch() -> (KillSwitch, Arc<Calls>) {
        let calls = Arc::new(Calls::default());
        let firewall = FakeFirewall {
            calls: calls.clone(),
        };
        (KillSwitch::new(Box::new(firewall)), calls)
    }

    #[test]
    fn enable_is_idempotent() {
        let (mut switch, calls) = kill_switch();
        switch.sync(true).unwrap();
        switch.sync(true).unwrap();
        assert_eq!(calls.enable.load(Ordering::SeqCst), 1);
        assert!(switch.status().enabled);
    }

    #[test]
    fn disable_is_idempotent() {
        let (mut switch, calls) = kill_switch();
        switch.sync(true).unwrap();
        switch.sync(false).unwrap();
        switch.sync(false).unwrap();
        assert_eq!(calls.disable.load(Ordering::SeqCst), 1);
        assert!(!switch.status().enabled);
    }

    #[test]
    fn enable_error_is_reported_and_retryable() {
        let (mut switch, calls) = kill_switch();
        calls.fail_enable.store(1, Ordering::SeqCst);
        let error = switch.sync(true).unwrap_err();
        assert!(error.contains("нет прав"));
        assert!(!switch.status().enabled);
        assert_eq!(switch.status().error.as_deref(), Some("нет прав"));

        switch.sync(true).unwrap();
        assert!(switch.status().enabled);
        assert!(switch.status().error.is_none());
    }

    #[test]
    fn failed_disable_keeps_protection_marked_as_enabled() {
        let (mut switch, calls) = kill_switch();
        switch.sync(true).unwrap();
        calls.fail_disable.store(1, Ordering::SeqCst);
        assert!(switch.sync(false).is_err());
        assert!(
            switch.status().enabled,
            "если снять правила не удалось, защита считается действующей"
        );
        assert_eq!(
            switch.status().error.as_deref(),
            Some("не удалось снять правила")
        );
    }

    #[test]
    fn release_is_noop_when_disabled() {
        let (mut switch, calls) = kill_switch();
        switch.release();
        assert_eq!(calls.disable.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn release_clears_active_state() {
        let (mut switch, calls) = kill_switch();
        switch.sync(true).unwrap();
        switch.release();
        switch.release();
        assert_eq!(calls.disable.load(Ordering::SeqCst), 1);
        assert!(!switch.status().enabled);
    }
}
