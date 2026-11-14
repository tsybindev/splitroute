//! Состояние helper'а и управление процессом ядра.
//!
//! Helper не принимает пути от клиента: конфиг пишется только в собственный
//! runtime-каталог с правами `0600`, перед запуском выполняется
//! `sing-box check`, а содержимое конфига не попадает в логи.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use splitroute_protocol::{CoreState, ErrorCode, HelperStatus};

#[cfg(all(test, unix))]
use crate::killswitch::Firewall;
use crate::killswitch::KillSwitch;
use crate::platform;

/// Пауза после запуска ядра: если процесс завершился, `start` сообщает ошибку.
const START_GRACE: Duration = Duration::from_millis(300);
/// Сколько ждём завершения ядра по SIGTERM перед SIGKILL.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Ошибка helper'а: стабильный код протокола и русский текст для пользователя.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperError {
    pub code: ErrorCode,
    pub message: String,
}

impl HelperError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn io(code: ErrorCode, context: &str, error: io::Error) -> Self {
        Self::new(code, format!("{context}: {error}"))
    }
}

/// Один процесс ядра и один runtime-каталог.
pub struct HelperCore {
    sing_box: PathBuf,
    runtime_dir: PathBuf,
    config_path: PathBuf,
    log_path: PathBuf,
    version: Option<String>,
    child: Option<Child>,
    started_at: Option<u64>,
    /// Конфиг записан этим экземпляром и должен быть удалён при остановке.
    config_owned: bool,
    /// Правила fail-closed; живут дольше процесса ядра.
    kill_switch: KillSwitch,
}

impl HelperCore {
    /// Создаёт runtime-каталог и определяет версию ядра.
    ///
    /// Каталог доступен на обход (`0711`), чтобы пользователь мог подключиться
    /// к сокету, но не мог прочитать его содержимое. Под обычным пользователем
    /// каталог остаётся `0700`.
    pub fn new(sing_box: impl Into<PathBuf>, runtime_dir: impl Into<PathBuf>) -> io::Result<Self> {
        let sing_box = sing_box.into();
        let runtime_dir = runtime_dir.into();

        fs::create_dir_all(&runtime_dir)?;
        platform::set_dir_mode(&runtime_dir)?;

        let version = read_version(&sing_box);
        let config_path = runtime_dir.join("config.json");
        let log_path = runtime_dir.join("sing-box.log");
        let kill_switch = KillSwitch::platform(&runtime_dir, &sing_box);

        Ok(Self {
            sing_box,
            runtime_dir,
            config_path,
            log_path,
            version,
            child: None,
            started_at: None,
            config_owned: false,
            kill_switch,
        })
    }

    /// Конструктор для тестов: firewall подменяется, ядро остаётся фиктивным.
    #[cfg(all(test, unix))]
    fn with_firewall(
        sing_box: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
        firewall: Box<dyn Firewall>,
    ) -> io::Result<Self> {
        let mut core = Self::new(sing_box, runtime_dir)?;
        core.kill_switch = KillSwitch::new(firewall);
        Ok(core)
    }

    /// Текущий статус; завершившийся процесс ядра замечается здесь же.
    pub fn status(&mut self) -> HelperStatus {
        self.reap();
        let running = self.child.is_some();
        HelperStatus {
            state: if running {
                CoreState::Running
            } else {
                CoreState::Stopped
            },
            pid: self.child.as_ref().map(Child::id),
            version: self.version.clone(),
            started_at: if running { self.started_at } else { None },
            // При падении ядра правила остаются: это и есть fail-closed.
            kill_switch: self.kill_switch.status(),
        }
    }

    /// Статус в виде JSON-объекта для ответа протокола.
    pub fn status_value(&mut self) -> Value {
        serde_json::to_value(self.status()).unwrap_or_else(|_| json!({ "state": "stopped" }))
    }

    /// Проверяет конфиг и запускает ядро. Повторный запуск запрещён.
    ///
    /// При `kill_switch = true` правила ставятся до запуска ядра. Если старт не
    /// удался, а правила поставил именно этот вызов, они снимаются: не блокируем
    /// сеть пользователя без работающего туннеля. Если правила остались от
    /// прошлой сессии после падения ядра, они сохраняются (fail-closed).
    pub fn start(&mut self, config: Value, kill_switch: bool) -> Result<HelperStatus, HelperError> {
        self.reap();
        if self.child.is_some() {
            return Err(HelperError::new(
                ErrorCode::AlreadyRunning,
                "ядро уже запущено",
            ));
        }
        if !config.is_object() {
            return Err(HelperError::new(
                ErrorCode::ConfigInvalid,
                "payload.config должен быть объектом",
            ));
        }

        let was_active = self.kill_switch.is_active();
        if let Err(error) = self.kill_switch.sync(kill_switch) {
            return Err(HelperError::new(
                ErrorCode::KillSwitchFailed,
                format!("не удалось применить kill switch: {error}"),
            ));
        }

        self.write_config(&config)
            .inspect_err(|_| self.rollback_kill_switch(was_active))?;

        let check = match Command::new(&self.sing_box)
            .arg("check")
            .arg("-c")
            .arg(&self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) => status,
            Err(error) => {
                self.discard_config();
                self.rollback_kill_switch(was_active);
                return Err(HelperError::io(
                    ErrorCode::StartFailed,
                    "не удалось проверить конфиг",
                    error,
                ));
            }
        };
        if !check.success() {
            self.discard_config();
            self.rollback_kill_switch(was_active);
            // Текст ошибки ядра может содержать значения конфига — не отдаём его.
            return Err(HelperError::new(
                ErrorCode::ConfigInvalid,
                "sing-box check не прошёл",
            ));
        }

        let log = match platform::open_log(&self.log_path) {
            Ok(log) => log,
            Err(error) => {
                self.discard_config();
                self.rollback_kill_switch(was_active);
                return Err(HelperError::io(
                    ErrorCode::StartFailed,
                    "не удалось открыть лог ядра",
                    error,
                ));
            }
        };
        let stdout = match log.try_clone() {
            Ok(stdout) => stdout,
            Err(error) => {
                self.discard_config();
                self.rollback_kill_switch(was_active);
                return Err(HelperError::io(
                    ErrorCode::StartFailed,
                    "не удалось подготовить лог ядра",
                    error,
                ));
            }
        };

        let mut child = match Command::new(&self.sing_box)
            .arg("run")
            .arg("-c")
            .arg(&self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(log))
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                self.discard_config();
                self.rollback_kill_switch(was_active);
                return Err(HelperError::io(
                    ErrorCode::StartFailed,
                    "не удалось запустить ядро",
                    error,
                ));
            }
        };

        std::thread::sleep(START_GRACE);
        if let Ok(Some(status)) = child.try_wait() {
            self.discard_config();
            self.rollback_kill_switch(was_active);
            let code = status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "сигнал".to_string());
            return Err(HelperError::new(
                ErrorCode::StartFailed,
                format!("ядро завершилось сразу после запуска (код {code})"),
            ));
        }

        self.child = Some(child);
        self.started_at = Some(now_secs());
        Ok(self.status())
    }

    /// Снимает правила, поставленные этим вызовом `start`.
    fn rollback_kill_switch(&mut self, was_active: bool) {
        if !was_active && self.kill_switch.is_active() {
            self.kill_switch.release();
        }
    }

    /// Останавливает ядро и снимает правила; операция идемпотентна.
    pub fn stop(&mut self) -> HelperStatus {
        self.stop_process();
        self.discard_config();
        self.kill_switch.release();
        self.status()
    }

    /// Пишет конфиг атомарно и только с правами `0600`.
    fn write_config(&mut self, config: &Value) -> Result<(), HelperError> {
        let temp_path = self.runtime_dir.join("config.json.tmp");
        let result = (|| -> io::Result<()> {
            let mut file = platform::open_private(&temp_path)?;
            serde_json::to_writer_pretty(&mut file, config).map_err(io::Error::other)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp_path, &self.config_path)?;
            platform::set_file_mode(&self.config_path)?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
            return result.map_err(|error| {
                HelperError::io(
                    ErrorCode::ConfigInvalid,
                    "не удалось записать конфиг",
                    error,
                )
            });
        }

        self.config_owned = true;
        Ok(())
    }

    fn discard_config(&mut self) {
        if !self.config_owned {
            let _ = fs::remove_file(self.runtime_dir.join("config.json.tmp"));
            return;
        }
        let _ = fs::remove_file(&self.config_path);
        let _ = fs::remove_file(self.runtime_dir.join("config.json.tmp"));
        self.config_owned = false;
    }

    fn reap(&mut self) {
        let exited = match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_)) | Err(_)),
            None => false,
        };
        if exited {
            self.child = None;
            self.started_at = None;
            self.discard_config();
        }
    }

    fn stop_process(&mut self) {
        let Some(mut child) = self.child.take() else {
            self.started_at = None;
            return;
        };
        self.started_at = None;
        platform::terminate(&mut child, STOP_TIMEOUT);
    }
}

impl Drop for HelperCore {
    fn drop(&mut self) {
        self.stop_process();
        self.discard_config();
        // При завершении helper'а правила не должны «залипнуть».
        self.kill_switch.release();
    }
}

fn read_version(sing_box: &Path) -> Option<String> {
    let output = Command::new(sing_box)
        .arg("version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct FakeCounters {
        enable: AtomicUsize,
        disable: AtomicUsize,
    }

    struct FakeFirewall {
        counters: Arc<FakeCounters>,
        fail_enable: bool,
    }

    impl Firewall for FakeFirewall {
        fn enable(&self) -> Result<(), String> {
            if self.fail_enable {
                return Err("нужны права root".to_string());
            }
            self.counters.enable.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn disable(&self) -> Result<(), String> {
            self.counters.disable.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn core_with_fake(dir: &Path, counters: Arc<FakeCounters>, fail_enable: bool) -> HelperCore {
        HelperCore::with_firewall(
            dir.join("missing-sing-box"),
            dir,
            Box::new(FakeFirewall {
                counters,
                fail_enable,
            }),
        )
        .unwrap()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("sr-core-{tag}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn config_file_is_written_with_private_permissions() {
        let dir = temp_dir("config");
        let mut core = HelperCore::new(dir.join("missing-sing-box"), &dir).unwrap();
        core.write_config(&json!({ "log": { "level": "info" } }))
            .unwrap();

        let mode = fs::metadata(&core.config_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&core.config_path).unwrap()).unwrap();
        assert_eq!(written["log"]["level"], "info");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_object_config_is_rejected_before_writing() {
        let dir = temp_dir("invalid");
        let mut core = HelperCore::new(dir.join("missing-sing-box"), &dir).unwrap();
        let error = core.start(Value::Null, false).unwrap_err();
        assert_eq!(error.code, ErrorCode::ConfigInvalid);
        assert!(!core.config_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_switch_failure_does_not_start_core() {
        let dir = temp_dir("ks-fail");
        let counters = Arc::new(FakeCounters::default());
        let mut core = core_with_fake(&dir, counters.clone(), true);

        let error = core
            .start(json!({ "log": { "level": "info" } }), true)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::KillSwitchFailed);
        assert!(error.message.contains("нужны права root"));
        assert!(!core.config_path.exists(), "конфиг не пишется без защиты");
        assert!(!core.status().kill_switch.enabled);
        assert_eq!(counters.enable.load(Ordering::SeqCst), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_start_rolls_back_rules_it_installed() {
        let dir = temp_dir("ks-rollback");
        let counters = Arc::new(FakeCounters::default());
        let mut core = core_with_fake(&dir, counters.clone(), false);

        // Ядра нет: проверка конфига падает после установки правил.
        let error = core
            .start(json!({ "log": { "level": "info" } }), true)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StartFailed);
        assert_eq!(counters.enable.load(Ordering::SeqCst), 1);
        assert_eq!(
            counters.disable.load(Ordering::SeqCst),
            1,
            "правила, поставленные этим вызовом, снимаются"
        );
        assert!(!core.status().kill_switch.enabled);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_start_keeps_protection_from_previous_crash() {
        let dir = temp_dir("ks-crash");
        let counters = Arc::new(FakeCounters::default());
        let mut core = core_with_fake(&dir, counters.clone(), false);
        core.kill_switch.sync(true).unwrap();

        let error = core
            .start(json!({ "log": { "level": "info" } }), true)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StartFailed);
        assert_eq!(
            counters.disable.load(Ordering::SeqCst),
            0,
            "после падения ядра правила должны оставаться"
        );
        assert!(core.status().kill_switch.enabled);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stop_removes_kill_switch_rules() {
        let dir = temp_dir("ks-stop");
        let counters = Arc::new(FakeCounters::default());
        let mut core = core_with_fake(&dir, counters.clone(), false);
        core.kill_switch.sync(true).unwrap();

        let status = core.stop();
        assert_eq!(counters.disable.load(Ordering::SeqCst), 1);
        assert!(!status.kill_switch.enabled);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn runtime_dir_is_private_for_regular_user() {
        let dir = temp_dir("runtime");
        let core = HelperCore::new(dir.join("missing-sing-box"), &dir).unwrap();
        let mode = fs::metadata(&core.runtime_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let expected = if platform::is_root() { 0o711 } else { 0o700 };
        assert_eq!(mode, expected);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fresh_core_does_not_remove_foreign_config() {
        let dir = temp_dir("foreign-config");
        let config_path = dir.join("config.json");
        fs::write(&config_path, "{}").unwrap();
        {
            let _core = HelperCore::new(dir.join("missing-sing-box"), &dir).unwrap();
        }
        assert!(
            config_path.exists(),
            "второй экземпляр не должен удалять конфиг работающего helper'а"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
