//! Linux: установка и удаление systemd-юнита helper'а.
//!
//! Бинарь helper'а и pinned sing-box кладутся в `/usr/lib/splitroute`, юнит — в
//! `/etc/systemd/system`. Сокет и runtime-каталог — `/run/splitroute`, каталог
//! создаётся самим systemd (`RuntimeDirectory=splitroute`,
//! `RuntimeDirectoryMode=0711`) и дополнительно готовится скриптом установки.
//!
//! Привилегированные шаги выполняет `pkexec /bin/sh <скрипт>`: подтверждение
//! даёт системный polkit-агент, установщик пароль не читает и не хранит.
//! Скрипты пишутся во временный каталог пользователя и содержат только
//! фиксированные пути и пути из `InstallPaths` — пользовательский ввод в них
//! не попадает.
//!
//! Модуль собирается и на macOS в тестовой конфигурации: генерация юнита и
//! скриптов — чистые функции, поэтому их можно проверить без Linux-хоста.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{HelperInstaller, InstallerError};

/// Имя systemd-юнита helper'а.
pub const HELPER_UNIT: &str = "splitroute-helper.service";

const HELPER_DIR: &str = "/usr/lib/splitroute";
const HELPER_BINARY: &str = "/usr/lib/splitroute/splitroute-helper";
const SING_BOX_BINARY: &str = "/usr/lib/splitroute/sing-box";
const UNIT_PATH: &str = "/etc/systemd/system/splitroute-helper.service";
const RUNTIME_DIR: &str = "/run/splitroute";
const SOCKET_PATH: &str = "/run/splitroute/helper.sock";

/// Куда и с какими правами ставится helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    pub helper: PathBuf,
    pub sing_box: PathBuf,
    pub unit: PathBuf,
    pub runtime_dir: PathBuf,
    pub socket: PathBuf,
    pub allowed_uid: u32,
}

impl InstallPaths {
    /// Стандартная раскладка для текущего пользователя.
    pub fn standard(allowed_uid: u32) -> Self {
        Self {
            helper: PathBuf::from(HELPER_BINARY),
            sing_box: PathBuf::from(SING_BOX_BINARY),
            unit: PathBuf::from(UNIT_PATH),
            runtime_dir: PathBuf::from(RUNTIME_DIR),
            socket: PathBuf::from(SOCKET_PATH),
            allowed_uid,
        }
    }
}

/// Пути к файлам, подготовленным во временном каталоге.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedPaths {
    pub helper: PathBuf,
    pub sing_box: PathBuf,
    pub unit: PathBuf,
}

/// Linux-установщик systemd-юнита.
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxInstaller;

impl LinuxInstaller {
    pub fn new() -> Self {
        Self
    }
}

impl HelperInstaller for LinuxInstaller {
    fn install(&self) -> Result<(), InstallerError> {
        let helper_source = resolve_helper_binary()?;
        let sing_box_source = resolve_sing_box_binary()?;
        let staging = Staging::new()?;

        let staged_helper = staging.path.join("splitroute-helper");
        let staged_sing_box = staging.path.join("sing-box");
        let staged_unit = staging.path.join(HELPER_UNIT);

        copy_binary(&helper_source, &staged_helper)?;
        copy_binary(&sing_box_source, &staged_sing_box)?;

        let paths = InstallPaths::standard(current_uid()?);
        fs::write(&staged_unit, unit_contents(&paths)).map_err(|error| {
            InstallerError::Failed(format!("не удалось записать юнит: {error}"))
        })?;
        fs::set_permissions(&staged_unit, fs::Permissions::from_mode(0o644))
            .map_err(|error| InstallerError::Failed(format!("юнит: {error}")))?;

        let staged = StagedPaths {
            helper: staged_helper,
            sing_box: staged_sing_box,
            unit: staged_unit,
        };
        let script = install_script(&paths, &staged);
        run_privileged_script(&staging.path, "install", &script)?;
        wait_for_socket(&paths.socket, Duration::from_secs(10))?;
        Ok(())
    }

    fn uninstall(&self) -> Result<(), InstallerError> {
        let paths = InstallPaths::standard(current_uid()?);
        let staging = Staging::new()?;
        run_privileged_script(&staging.path, "uninstall", &uninstall_script(&paths))
    }
}

/// Содержимое systemd-юнита helper'а.
///
/// Юнит работает от root: helper принимает только фиксированные команды и сам
/// пишет конфиг ядра. `--allow-uid` ограничивает сокет одним пользователем.
pub fn unit_contents(paths: &InstallPaths) -> String {
    format!(
        "[Unit]\n\
         Description=SplitRoute privileged helper\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User=root\n\
         RuntimeDirectory=splitroute\n\
         RuntimeDirectoryMode=0711\n\
         UMask=0077\n\
         ExecStart={helper} --socket {socket} --runtime-dir {runtime} --sing-box {sing_box} --allow-uid {uid}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        helper = systemd_quote(&paths.helper.to_string_lossy()),
        socket = systemd_quote(&paths.socket.to_string_lossy()),
        runtime = systemd_quote(&paths.runtime_dir.to_string_lossy()),
        sing_box = systemd_quote(&paths.sing_box.to_string_lossy()),
        uid = paths.allowed_uid,
    )
}

/// Привилегированный скрипт установки (запускается через `pkexec /bin/sh`).
pub fn install_script(paths: &InstallPaths, staged: &StagedPaths) -> String {
    format!(
        "set -eu\n\
         /bin/mkdir -p {helper_dir} {runtime}\n\
         /bin/chmod 0711 {runtime}\n\
         /usr/bin/install -m 0755 -o root -g root {staged_helper} {helper}\n\
         /usr/bin/install -m 0755 -o root -g root {staged_sing_box} {sing_box}\n\
         /usr/bin/install -m 0644 -o root -g root {staged_unit} {unit}\n\
         /bin/systemctl disable --now {service} >/dev/null 2>&1 || true\n\
         /bin/systemctl daemon-reload\n\
         /bin/systemctl enable --now {service}\n",
        helper_dir = shell_quote(HELPER_DIR),
        runtime = shell_quote(&paths.runtime_dir.to_string_lossy()),
        staged_helper = shell_quote(&staged.helper.to_string_lossy()),
        staged_sing_box = shell_quote(&staged.sing_box.to_string_lossy()),
        staged_unit = shell_quote(&staged.unit.to_string_lossy()),
        helper = shell_quote(&paths.helper.to_string_lossy()),
        sing_box = shell_quote(&paths.sing_box.to_string_lossy()),
        unit = shell_quote(&paths.unit.to_string_lossy()),
        service = HELPER_UNIT,
    )
}

/// Привилегированный скрипт удаления: сервис, бинари, юнит, runtime-каталог.
pub fn uninstall_script(paths: &InstallPaths) -> String {
    format!(
        "set -eu\n\
         /bin/systemctl disable --now {service} >/dev/null 2>&1 || true\n\
         /bin/rm -f {unit}\n\
         /bin/rm -f {helper}\n\
         /bin/rm -f {sing_box}\n\
         /bin/rmdir {helper_dir} >/dev/null 2>&1 || true\n\
         /bin/systemctl daemon-reload\n\
         /bin/rm -rf {runtime}\n",
        service = HELPER_UNIT,
        unit = shell_quote(&paths.unit.to_string_lossy()),
        helper = shell_quote(&paths.helper.to_string_lossy()),
        sing_box = shell_quote(&paths.sing_box.to_string_lossy()),
        helper_dir = shell_quote(HELPER_DIR),
        runtime = shell_quote(&paths.runtime_dir.to_string_lossy()),
    )
}

/// Экранирование для одинарных кавычек POSIX shell.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Экранирование пути в строке юнита systemd.
///
/// Значения формирует сам установщик, но `InstallPaths` публична: подстрахуемся
/// от пробелов, кавычек и спецификаторов `%` в путях.
fn systemd_quote(value: &str) -> String {
    let plain = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._/:@+".contains(c));
    if plain {
        return value.to_string();
    }
    format!(
        "\"{}\"",
        value
            .replace('%', "%%")
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    )
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn require_file(path: PathBuf) -> Result<PathBuf, InstallerError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(InstallerError::MissingBinary(path.display().to_string()))
    }
}

fn first_existing(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| path.is_file())
}

/// Triple для имён sidecar-бинарей Tauri (`scripts/fetch-core.sh`).
fn linux_triple() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64-unknown-linux-gnu".to_string(),
        "aarch64" => "aarch64-unknown-linux-gnu".to_string(),
        other => format!("{other}-unknown-linux-gnu"),
    }
}

fn executable_dir() -> Result<PathBuf, InstallerError> {
    let executable = std::env::current_exe().map_err(|error| {
        InstallerError::Failed(format!("не удалось определить путь приложения: {error}"))
    })?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| InstallerError::MissingBinary("каталог приложения не определён".to_string()))
}

fn find_helper_in(directory: &Path) -> Option<PathBuf> {
    first_existing([
        directory.join("splitroute-helper"),
        directory.join(format!("splitroute-helper-{}", linux_triple())),
    ])
}

fn find_sing_box_in(directory: &Path) -> Option<PathBuf> {
    let triple = linux_triple();
    first_existing([
        directory.join(format!("sing-box-{triple}")),
        directory.join("sing-box"),
    ])
}

/// Бинарь helper'а: env-переопределение или сосед основного исполняемого файла.
fn resolve_helper_binary() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_HELPER_BIN") {
        return require_file(path);
    }
    let directory = executable_dir()?;
    find_helper_in(&directory).ok_or_else(|| {
        InstallerError::MissingBinary(
            "splitroute-helper рядом с приложением; соберите `cargo build -p splitroute-helper`"
                .to_string(),
        )
    })
}

/// Pinned sing-box: env-переопределение, sidecar приложения или dev-каталог.
fn resolve_sing_box_binary() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_SING_BOX_BIN") {
        return require_file(path);
    }

    let directory = executable_dir()?;
    if let Some(path) = find_sing_box_in(&directory) {
        return Ok(path);
    }

    // Dev-сборка: target/debug -> корень репозитория -> src-tauri/binaries.
    if let Some(root) = directory.parent().and_then(Path::parent) {
        let candidate = root.join("src-tauri").join("binaries");
        if let Some(path) = find_sing_box_in(&candidate) {
            return Ok(path);
        }
    }

    Err(InstallerError::MissingBinary(
        "sing-box; выполните ./scripts/fetch-core.sh".to_string(),
    ))
}

fn copy_binary(source: &Path, destination: &Path) -> Result<(), InstallerError> {
    fs::copy(source, destination).map_err(|error| {
        InstallerError::Failed(format!(
            "не удалось скопировать {}: {error}",
            source.display()
        ))
    })?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o755)).map_err(|error| {
        InstallerError::Failed(format!("права на {}: {error}", destination.display()))
    })
}

/// uid пользователя, которому разрешено подключаться к сокету.
///
/// Берём реальный uid из `/proc/self/status`: лишняя зависимость (`libc`) не
/// нужна, а приложение запускается от того же пользователя, что и клиент.
fn current_uid() -> Result<u32, InstallerError> {
    let status = fs::read_to_string("/proc/self/status").map_err(|error| {
        InstallerError::Failed(format!("не удалось прочитать /proc/self/status: {error}"))
    })?;
    parse_status_uid(&status)
        .ok_or_else(|| InstallerError::Failed("в /proc/self/status нет строки Uid".to_string()))
}

fn parse_status_uid(status: &str) -> Option<u32> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix("Uid:")?;
        value.split_whitespace().next()?.parse().ok()
    })
}

fn pkexec_path() -> Result<PathBuf, InstallerError> {
    first_existing([
        PathBuf::from("/usr/bin/pkexec"),
        PathBuf::from("/bin/pkexec"),
    ])
    .ok_or_else(|| InstallerError::Failed("не найден pkexec; установите polkit".to_string()))
}

fn run_privileged_script(directory: &Path, name: &str, script: &str) -> Result<(), InstallerError> {
    let pkexec = pkexec_path()?;
    let script_path = directory.join(format!("{name}.sh"));
    fs::write(&script_path, script)
        .map_err(|error| InstallerError::Failed(format!("скрипт установки: {error}")))?;
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700))
        .map_err(|error| InstallerError::Failed(format!("права на скрипт: {error}")))?;

    let output = Command::new(pkexec)
        .arg("/bin/sh")
        .arg(&script_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| InstallerError::Failed(format!("pkexec: {error}")))?;
    if output.status.success() {
        return Ok(());
    }

    // pkexec: 126 — диалог polkit закрыт пользователем, 127 — нет прав.
    match output.status.code() {
        Some(126) => Err(InstallerError::Failed(
            "операция отменена пользователем".to_string(),
        )),
        Some(127) => Err(InstallerError::RequiresPrivileges),
        _ => Err(InstallerError::Failed(format!(
            "pkexec завершился с ошибкой: {}",
            short_output(&output.stderr)
        ))),
    }
}

fn short_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "нет вывода".to_string();
    }
    trimmed.chars().take(300).collect()
}

fn wait_for_socket(path: &Path, timeout: Duration) -> Result<(), InstallerError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(InstallerError::Failed(format!(
        "helper не создал сокет {}",
        path.display()
    )))
}

struct Staging {
    path: PathBuf,
}

impl Staging {
    fn new() -> Result<Self, InstallerError> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "splitroute-install-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path)
            .map_err(|error| InstallerError::Failed(format!("временный каталог: {error}")))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            InstallerError::Failed(format!("права на временный каталог: {error}"))
        })?;
        Ok(Self { path })
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> InstallPaths {
        InstallPaths::standard(1000)
    }

    fn staged() -> StagedPaths {
        StagedPaths {
            helper: PathBuf::from("/tmp/stage/splitroute-helper"),
            sing_box: PathBuf::from("/tmp/stage/sing-box"),
            unit: PathBuf::from("/tmp/stage/splitroute-helper.service"),
        }
    }

    fn temp_dir(stem: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("sr-linux-{stem}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn unit_has_required_directives() {
        let unit = unit_contents(&paths());
        assert!(unit.starts_with("[Unit]\n"));
        assert!(unit.contains("Description="));
        assert!(unit.contains("After=network-online.target"));
        assert!(unit.contains("Type=simple"));
        assert!(unit.contains("User=root"));
        assert!(unit.contains("RuntimeDirectory=splitroute"));
        assert!(unit.contains("RuntimeDirectoryMode=0711"));
        assert!(unit.contains("UMask=0077"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("RestartSec=2"));
        assert!(unit.contains("WantedBy=multi-user.target"));
        assert!(unit.trim_end().ends_with("WantedBy=multi-user.target"));
    }

    #[test]
    fn exec_start_uses_packaged_paths_and_uid() {
        let unit = unit_contents(&paths());
        assert!(unit.contains(
            "ExecStart=/usr/lib/splitroute/splitroute-helper --socket /run/splitroute/helper.sock \
             --runtime-dir /run/splitroute --sing-box /usr/lib/splitroute/sing-box \
             --allow-uid 1000\n"
        ));
        assert!(unit.contains("/run/splitroute/helper.sock"));
        assert!(unit.contains("--allow-uid 1000"));
    }

    #[test]
    fn standard_paths_match_install_layout() {
        let paths = paths();
        assert_eq!(
            paths.helper,
            PathBuf::from("/usr/lib/splitroute/splitroute-helper")
        );
        assert_eq!(
            paths.sing_box,
            PathBuf::from("/usr/lib/splitroute/sing-box")
        );
        assert_eq!(
            paths.unit,
            PathBuf::from("/etc/systemd/system/splitroute-helper.service")
        );
        assert_eq!(paths.socket, PathBuf::from("/run/splitroute/helper.sock"));
        assert_eq!(paths.runtime_dir, PathBuf::from("/run/splitroute"));
    }

    #[test]
    fn install_script_installs_fixed_paths_and_starts_service() {
        let script = install_script(&paths(), &staged());
        assert!(script.starts_with("set -eu"));
        assert!(script.contains("/usr/bin/install -m 0755 -o root -g root"));
        assert!(script.contains("'/usr/lib/splitroute/splitroute-helper'"));
        assert!(script.contains("'/usr/lib/splitroute/sing-box'"));
        assert!(script.contains("'/etc/systemd/system/splitroute-helper.service'"));
        assert!(script.contains("/bin/systemctl disable --now splitroute-helper.service"));
        assert!(script.contains("/bin/systemctl daemon-reload"));
        assert!(script.contains("/bin/systemctl enable --now splitroute-helper.service"));
        assert!(script.contains("chmod 0711 '/run/splitroute'"));
        // Скрипт не должен тянуть произвольные пути из окружения.
        assert!(!script.contains('$'));
    }

    #[test]
    fn uninstall_script_removes_service_and_runtime_only() {
        let script = uninstall_script(&paths());
        assert!(script.starts_with("set -eu"));
        assert!(script.contains("/bin/systemctl disable --now splitroute-helper.service"));
        assert!(script.contains("/bin/rm -f '/etc/systemd/system/splitroute-helper.service'"));
        assert!(script.contains("/bin/rm -f '/usr/lib/splitroute/splitroute-helper'"));
        assert!(script.contains("/bin/rm -f '/usr/lib/splitroute/sing-box'"));
        assert!(script.contains("/bin/systemctl daemon-reload"));
        assert!(script.contains("/bin/rm -rf '/run/splitroute'"));
        assert!(!script.contains("rm -rf '/usr"));
        assert!(!script.contains("rm -rf '/etc"));
    }

    #[test]
    fn generated_shell_scripts_parse() {
        for (name, script) in [
            ("install", install_script(&paths(), &staged())),
            ("uninstall", uninstall_script(&paths())),
        ] {
            let directory = temp_dir(name);
            let path = directory.join(format!("{name}.sh"));
            fs::write(&path, script).unwrap();
            let status = Command::new("/bin/sh")
                .arg("-n")
                .arg(&path)
                .status()
                .unwrap();
            let _ = fs::remove_dir_all(&directory);
            assert!(status.success(), "{name}.sh не проходит sh -n");
        }
    }

    #[test]
    fn missing_binaries_are_reported() {
        let directory = temp_dir("empty");
        assert!(find_helper_in(&directory).is_none());
        assert!(find_sing_box_in(&directory).is_none());

        let error = require_file(directory.join("sing-box")).unwrap_err();
        let _ = fs::remove_dir_all(&directory);
        assert!(matches!(error, InstallerError::MissingBinary(_)));
    }

    #[test]
    fn present_binary_is_found_in_directory() {
        let directory = temp_dir("present");
        let helper = directory.join("splitroute-helper");
        fs::write(&helper, b"#!/bin/sh\n").unwrap();
        assert_eq!(find_helper_in(&directory), Some(helper));

        let sing_box = directory.join(format!("sing-box-{}", linux_triple()));
        fs::write(&sing_box, b"#!/bin/sh\n").unwrap();
        assert_eq!(find_sing_box_in(&directory), Some(sing_box));

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn linux_triple_matches_arch() {
        let triple = linux_triple();
        assert!(triple.ends_with("-unknown-linux-gnu"));
        assert!(triple.starts_with(std::env::consts::ARCH));
    }

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn systemd_quote_escapes_unsafe_paths() {
        assert_eq!(
            systemd_quote("/usr/lib/splitroute/sing-box"),
            "/usr/lib/splitroute/sing-box"
        );
        assert_eq!(systemd_quote("/opt/a b/sing-box"), "\"/opt/a b/sing-box\"");
        assert_eq!(systemd_quote("/opt/a\"b"), "\"/opt/a\\\"b\"");
        assert_eq!(
            systemd_quote("/opt/100%/sing-box"),
            "\"/opt/100%%/sing-box\""
        );
        assert_eq!(systemd_quote(""), "\"\"");
    }

    #[test]
    fn uid_is_parsed_from_proc_status() {
        let status =
            "Name:\tsplitroute\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(parse_status_uid(status), Some(1000));
        assert_eq!(parse_status_uid("Name:\tsplitroute\n"), None);
        assert_eq!(parse_status_uid("Uid:\tnot-a-number\n"), None);
    }
}
