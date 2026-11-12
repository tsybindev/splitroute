//! macOS: установка и удаление LaunchDaemon helper'а.
//!
//! Бинарь кладётся в `/Library/PrivilegedHelperTools`, sing-box — в
//! `/Library/Application Support/SplitRoute`, plist — в `/Library/LaunchDaemons`.
//! Сокет и runtime-каталог — `/var/run/splitroute`.
//!
//! Привилегированные шаги выполняет `osascript` с `administrator privileges`:
//! система сама показывает запрос пароля, установщик его не читает и не хранит.
//! Скрипты установки пишутся во временный каталог и содержат только
//! фиксированные пути и пути из разрешённых источников (бинари приложения).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{HelperInstaller, InstallerError};

/// Метка LaunchDaemon и имя plist.
pub const HELPER_LABEL: &str = "com.splitroute.helper";

const HELPER_DIR: &str = "/Library/PrivilegedHelperTools";
const HELPER_BINARY: &str = "/Library/PrivilegedHelperTools/com.splitroute.helper";
const SING_BOX_DIR: &str = "/Library/Application Support/SplitRoute";
const SING_BOX_BINARY: &str = "/Library/Application Support/SplitRoute/sing-box";
const PLIST_PATH: &str = "/Library/LaunchDaemons/com.splitroute.helper.plist";
const RUNTIME_DIR: &str = "/var/run/splitroute";
const SOCKET_PATH: &str = "/var/run/splitroute/helper.sock";

/// Куда и с какими правами ставится helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    pub helper: PathBuf,
    pub sing_box: PathBuf,
    pub plist: PathBuf,
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
            plist: PathBuf::from(PLIST_PATH),
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
    pub plist: PathBuf,
}

/// macOS-установщик LaunchDaemon.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacosInstaller;

impl MacosInstaller {
    pub fn new() -> Self {
        Self
    }
}

impl HelperInstaller for MacosInstaller {
    fn install(&self) -> Result<(), InstallerError> {
        let helper_source = resolve_helper_binary()?;
        let sing_box_source = resolve_sing_box_binary()?;
        let staging = Staging::new()?;

        let staged_helper = staging.path.join("splitroute-helper");
        let staged_sing_box = staging.path.join("sing-box");
        let staged_plist = staging.path.join("com.splitroute.helper.plist");

        copy_binary(&helper_source, &staged_helper)?;
        copy_binary(&sing_box_source, &staged_sing_box)?;
        ensure_signature(&staged_helper)?;

        let paths = InstallPaths::standard(current_uid());
        fs::write(&staged_plist, plist_contents(&paths)).map_err(|error| {
            InstallerError::Failed(format!("не удалось записать plist: {error}"))
        })?;
        fs::set_permissions(&staged_plist, fs::Permissions::from_mode(0o644))
            .map_err(|error| InstallerError::Failed(format!("plist: {error}")))?;

        let staged = StagedPaths {
            helper: staged_helper,
            sing_box: staged_sing_box,
            plist: staged_plist,
        };
        let script = install_script(&paths, &staged);
        run_privileged_script(&staging.path, "install", &script)?;
        wait_for_socket(&paths.socket, Duration::from_secs(10))?;
        Ok(())
    }

    fn uninstall(&self) -> Result<(), InstallerError> {
        let paths = InstallPaths::standard(current_uid());
        let staging = Staging::new()?;
        run_privileged_script(&staging.path, "uninstall", &uninstall_script(&paths))
    }
}

/// Содержимое plist для LaunchDaemon.
pub fn plist_contents(paths: &InstallPaths) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{helper}</string>
    <string>--socket</string>
    <string>{socket}</string>
    <string>--runtime-dir</string>
    <string>{runtime}</string>
    <string>--sing-box</string>
    <string>{sing_box}</string>
    <string>--allow-uid</string>
    <string>{uid}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
</dict>
</plist>
"#,
        label = xml_escape(HELPER_LABEL),
        helper = xml_escape(&paths.helper.to_string_lossy()),
        socket = xml_escape(&paths.socket.to_string_lossy()),
        runtime = xml_escape(&paths.runtime_dir.to_string_lossy()),
        sing_box = xml_escape(&paths.sing_box.to_string_lossy()),
        uid = paths.allowed_uid,
    )
}

/// Привилегированный скрипт установки.
pub fn install_script(paths: &InstallPaths, staged: &StagedPaths) -> String {
    format!(
        "set -eu\n\
         /bin/mkdir -p {helper_dir} {sing_box_dir} {runtime}\n\
         /bin/chmod 0711 {runtime}\n\
         /usr/bin/install -m 0755 -o root -g wheel {staged_helper} {helper}\n\
         /usr/bin/install -m 0755 -o root -g wheel {staged_sing_box} {sing_box}\n\
         /usr/bin/install -m 0644 -o root -g wheel {staged_plist} {plist}\n\
         /bin/launchctl bootout system/{label} >/dev/null 2>&1 || true\n\
         /bin/launchctl bootstrap system {plist}\n\
         /bin/launchctl enable system/{label}\n\
         /bin/launchctl kickstart -k system/{label}\n",
        helper_dir = shell_quote(HELPER_DIR),
        sing_box_dir = shell_quote(SING_BOX_DIR),
        runtime = shell_quote(RUNTIME_DIR),
        staged_helper = shell_quote(&staged.helper.to_string_lossy()),
        staged_sing_box = shell_quote(&staged.sing_box.to_string_lossy()),
        staged_plist = shell_quote(&staged.plist.to_string_lossy()),
        helper = shell_quote(&paths.helper.to_string_lossy()),
        sing_box = shell_quote(&paths.sing_box.to_string_lossy()),
        plist = shell_quote(&paths.plist.to_string_lossy()),
        label = HELPER_LABEL,
    )
}

/// Привилегированный скрипт удаления: сервис, бинари, runtime-каталог.
pub fn uninstall_script(paths: &InstallPaths) -> String {
    format!(
        "set -eu\n\
         /bin/launchctl bootout system/{label} >/dev/null 2>&1 || true\n\
         /bin/rm -f {plist}\n\
         /bin/rm -f {helper}\n\
         /bin/rm -f {sing_box}\n\
         /bin/rmdir {sing_box_dir} >/dev/null 2>&1 || true\n\
         /bin/rm -rf {runtime}\n",
        label = HELPER_LABEL,
        plist = shell_quote(&paths.plist.to_string_lossy()),
        helper = shell_quote(&paths.helper.to_string_lossy()),
        sing_box = shell_quote(&paths.sing_box.to_string_lossy()),
        sing_box_dir = shell_quote(SING_BOX_DIR),
        runtime = shell_quote(&paths.runtime_dir.to_string_lossy()),
    )
}

/// Экранирование для одинарных кавычек POSIX shell.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// AppleScript, который запускает подготовленный shell-скрипт от администратора.
///
/// Пароль запрашивает система; установщик его не видит и не хранит.
fn applescript_source(script_path: &Path) -> String {
    format!(
        "do shell script (\"/bin/sh \" & quoted form of \"{}\") with administrator privileges\n",
        applescript_escape(&script_path.to_string_lossy())
    )
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
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

/// Бинарь helper'а: env-переопределение или сосед основного исполняемого файла.
fn resolve_helper_binary() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_HELPER_BIN") {
        return require_file(path);
    }

    let executable = std::env::current_exe().map_err(|error| {
        InstallerError::Failed(format!("не удалось определить путь приложения: {error}"))
    })?;
    let candidate = executable
        .parent()
        .map(|directory| directory.join("splitroute-helper"));
    if let Some(path) = candidate.filter(|path| path.is_file()) {
        return Ok(path);
    }

    Err(InstallerError::MissingBinary(
        "splitroute-helper рядом с приложением; соберите `cargo build -p splitroute-helper`"
            .to_string(),
    ))
}

/// Pinned sing-box: env-переопределение, sidecar приложения или dev-каталог.
fn resolve_sing_box_binary() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_SING_BOX_BIN") {
        return require_file(path);
    }

    let triple = format!("{}-apple-darwin", std::env::consts::ARCH);
    let executable = std::env::current_exe().map_err(|error| {
        InstallerError::Failed(format!("не удалось определить путь приложения: {error}"))
    })?;
    let directory = executable.parent().ok_or_else(|| {
        InstallerError::MissingBinary("каталог приложения не определён".to_string())
    })?;

    for name in [format!("sing-box-{triple}"), "sing-box".to_string()] {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    // Dev-сборка: target/debug -> корень репозитория -> src-tauri/binaries.
    if let Some(root) = directory.parent().and_then(Path::parent) {
        let candidate = root
            .join("src-tauri")
            .join("binaries")
            .join(format!("sing-box-{triple}"));
        if candidate.is_file() {
            return Ok(candidate);
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

/// Подпись helper'а: уже подписанный бинарь не трогаем.
///
/// В debug неподписанный бинарь получает ad-hoc подпись; в release это ошибка —
/// для распространения нужна подпись Developer ID и notarization (R7).
fn ensure_signature(path: &Path) -> Result<(), InstallerError> {
    if is_signed(path) {
        return Ok(());
    }
    if cfg!(debug_assertions) {
        return adhoc_sign(path);
    }
    Err(InstallerError::Failed(
        "helper не подписан; для релиза нужна подпись Developer ID".to_string(),
    ))
}

fn is_signed(path: &Path) -> bool {
    Command::new("/usr/bin/codesign")
        .arg("--verify")
        .arg("--strict")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Ad-hoc подпись для dev-режима; релизная сборка подписывается Developer ID.
fn adhoc_sign(path: &Path) -> Result<(), InstallerError> {
    let output = Command::new("/usr/bin/codesign")
        .arg("--force")
        .arg("--sign")
        .arg("-")
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| InstallerError::Failed(format!("codesign: {error}")))?;
    if !output.status.success() {
        return Err(InstallerError::Failed(format!(
            "codesign завершился с ошибкой: {}",
            short_output(&output.stderr)
        )));
    }
    Ok(())
}

fn run_privileged_script(directory: &Path, name: &str, script: &str) -> Result<(), InstallerError> {
    let script_path = directory.join(format!("{name}.sh"));
    fs::write(&script_path, script)
        .map_err(|error| InstallerError::Failed(format!("скрипт установки: {error}")))?;
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700))
        .map_err(|error| InstallerError::Failed(format!("права на скрипт: {error}")))?;

    let applescript_path = directory.join(format!("{name}.applescript"));
    fs::write(&applescript_path, applescript_source(&script_path))
        .map_err(|error| InstallerError::Failed(format!("AppleScript: {error}")))?;

    let output = Command::new("/usr/bin/osascript")
        .arg(&applescript_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| InstallerError::Failed(format!("osascript: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("-128") || stderr.to_lowercase().contains("cancel") {
            return Err(InstallerError::Failed(
                "операция отменена пользователем".to_string(),
            ));
        }
        return Err(InstallerError::Failed(format!(
            "osascript завершился с ошибкой: {}",
            short_output(&output.stderr)
        )));
    }
    Ok(())
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
        InstallPaths::standard(501)
    }

    fn staged() -> StagedPaths {
        StagedPaths {
            helper: PathBuf::from("/tmp/stage/splitroute-helper"),
            sing_box: PathBuf::from("/tmp/stage/sing-box"),
            plist: PathBuf::from("/tmp/stage/com.splitroute.helper.plist"),
        }
    }

    #[test]
    fn plist_has_label_arguments_and_uid() {
        let plist = plist_contents(&paths());
        assert!(plist.contains("<string>com.splitroute.helper</string>"));
        assert!(plist.contains("<string>--socket</string>"));
        assert!(plist.contains("<string>/var/run/splitroute/helper.sock</string>"));
        assert!(plist.contains("<string>--allow-uid</string>"));
        assert!(plist.contains("<string>501</string>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
    }

    #[test]
    fn socket_matches_client_release_default() {
        assert_eq!(
            paths().socket,
            PathBuf::from("/var/run/splitroute/helper.sock")
        );
    }

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn install_script_installs_fixed_paths_and_starts_service() {
        let script = install_script(&paths(), &staged());
        assert!(script.starts_with("set -eu"));
        assert!(script.contains("/usr/bin/install -m 0755 -o root -g wheel"));
        assert!(script.contains("'/Library/PrivilegedHelperTools/com.splitroute.helper'"));
        assert!(script.contains("'/Library/LaunchDaemons/com.splitroute.helper.plist'"));
        assert!(script.contains("launchctl bootstrap system"));
        assert!(script.contains("launchctl kickstart -k system/com.splitroute.helper"));
        assert!(script.contains("chmod 0711 '/var/run/splitroute'"));
    }

    #[test]
    fn uninstall_script_removes_service_and_runtime_only() {
        let script = uninstall_script(&paths());
        assert!(script.contains("launchctl bootout system/com.splitroute.helper"));
        assert!(script.contains("rm -f '/Library/LaunchDaemons/com.splitroute.helper.plist'"));
        assert!(script.contains("rm -f '/Library/PrivilegedHelperTools/com.splitroute.helper'"));
        assert!(script.contains("rm -f '/Library/Application Support/SplitRoute/sing-box'"));
        assert!(script.contains("rm -rf '/var/run/splitroute'"));
        assert!(!script.contains("rm -rf '/Library"));
    }

    #[test]
    fn applescript_escape_handles_quotes_and_backslashes() {
        assert_eq!(applescript_escape(r#"/tmp/a"b\c"#), r#"/tmp/a\"b\\c"#);
    }

    #[test]
    fn xml_escape_handles_special_characters() {
        assert_eq!(xml_escape("a&b<c>\"d\""), "a&amp;b&lt;c&gt;&quot;d&quot;");
    }

    fn temp_path(stem: &str, extension: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sr-macos-{stem}-{}-{unique}.{extension}",
            std::process::id()
        ))
    }

    #[test]
    fn generated_shell_scripts_parse() {
        for (name, script) in [
            ("install", install_script(&paths(), &staged())),
            ("uninstall", uninstall_script(&paths())),
        ] {
            let path = temp_path(name, "sh");
            fs::write(&path, script).unwrap();
            let status = Command::new("/bin/sh")
                .arg("-n")
                .arg(&path)
                .status()
                .unwrap();
            let _ = fs::remove_file(&path);
            assert!(status.success(), "{name}.sh не проходит sh -n");
        }
    }

    #[test]
    fn applescript_source_compiles() {
        let script_path = temp_path("install", "sh");
        let source_path = temp_path("install", "applescript");
        let output_path = temp_path("install", "scpt");
        fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::write(&source_path, applescript_source(&script_path)).unwrap();

        let output = Command::new("/usr/bin/osacompile")
            .arg("-o")
            .arg(&output_path)
            .arg(&source_path)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        let _ = fs::remove_file(&script_path);
        let _ = fs::remove_file(&source_path);
        let _ = fs::remove_file(&output_path);
        assert!(output.status.success(), "osacompile: {stderr}");
    }
}
