//! Windows: установка и удаление службы helper'а.
//!
//! Служба `splitroute-helper-service` работает под SYSTEM и слушает named pipe
//! `\\.\pipe\splitroute-helper` с ACL на SID пользователя (см. `crates/helper`).
//! Установщик раскладывает файлы в `C:\Program Files\SplitRoute`, причём
//! `wintun.dll` лежит рядом с `sing-box.exe` — этого требует ядро. Runtime-каталог
//! (конфиг и логи ядра) — `C:\ProgramData\SplitRoute`.
//!
//! Привилегированные шаги выполняет bat-скрипт, запущенный через
//! `Start-Process -Verb RunAs`: подтверждение спрашивает UAC, установщик пароль
//! не читает и не хранит. Скрипт собирается только из фиксированных путей и
//! [`InstallPaths`]; пользовательский ввод в команды не попадает.
//!
//! Генераторы команд, путей и скриптов — чистые функции, поэтому их тесты
//! выполняются и на других платформах; `windows-sys` подключается только под
//! `cfg(windows)`, а исполнение скриптов дополнительно закрыто проверкой
//! [`cfg!(windows)`] — на прочих ОС установщик отвечает `Unsupported`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{HelperInstaller, InstallerError};

/// Имя службы; совпадает с `splitroute_helper::service::SERVICE_NAME`.
pub const SERVICE_NAME: &str = "splitroute-helper-service";

/// Имя named pipe без префикса; совпадает с аргументом `--pipe` helper'а.
pub const PIPE_NAME: &str = "splitroute-helper";

/// Отображаемое имя службы в оснастке «Службы».
const SERVICE_DISPLAY_NAME: &str = "SplitRoute helper";

const HELPER_BINARY: &str = r"C:\Program Files\SplitRoute\splitroute-helper.exe";
const SING_BOX_BINARY: &str = r"C:\Program Files\SplitRoute\sing-box.exe";
const WINTUN_LIBRARY: &str = r"C:\Program Files\SplitRoute\wintun.dll";
const RUNTIME_DIR: &str = r"C:\ProgramData\SplitRoute";

/// `ERROR_CANCELLED`: пользователь отказал в запросе UAC.
const UAC_CANCELLED_CODE: i32 = 1223;

/// Куда и с какими правами ставится helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    pub helper: PathBuf,
    /// Ядро; `wintun.dll` должно лежать в том же каталоге.
    pub sing_box: PathBuf,
    pub wintun: PathBuf,
    pub runtime_dir: PathBuf,
    /// SID пользователя, которому разрешено подключение к pipe (`S-1-5-...`).
    pub allowed_sid: String,
}

impl InstallPaths {
    /// Стандартная раскладка для указанного пользователя.
    pub fn standard(allowed_sid: &str) -> Self {
        Self {
            helper: PathBuf::from(HELPER_BINARY),
            sing_box: PathBuf::from(SING_BOX_BINARY),
            wintun: PathBuf::from(WINTUN_LIBRARY),
            runtime_dir: PathBuf::from(RUNTIME_DIR),
            allowed_sid: allowed_sid.to_string(),
        }
    }
}

/// Файлы, подготовленные во временном каталоге перед повышением прав.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFiles {
    pub helper: PathBuf,
    pub sing_box: PathBuf,
    pub wintun: PathBuf,
}

/// Windows-установщик службы helper'а.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsInstaller;

impl WindowsInstaller {
    pub fn new() -> Self {
        Self
    }

    /// Копирует бинари во временный каталог и запускает bat-скрипт от администратора.
    fn install_elevated(&self) -> Result<(), InstallerError> {
        let sid = current_user_sid()?;
        validate_sid(&sid)?;

        let helper_source = resolve_helper_binary()?;
        let sing_box_source = resolve_sing_box_binary()?;
        let wintun_source = resolve_wintun_library()?;

        let staging = Staging::new()?;
        let staged = StagedFiles {
            helper: staging.path.join("splitroute-helper.exe"),
            sing_box: staging.path.join("sing-box.exe"),
            wintun: staging.path.join("wintun.dll"),
        };
        copy_file(&helper_source, &staged.helper)?;
        copy_file(&sing_box_source, &staged.sing_box)?;
        copy_file(&wintun_source, &staged.wintun)?;

        let paths = InstallPaths::standard(&sid);
        let script_path = staging.path.join("install.bat");
        write_script(&script_path, &install_script(&paths, &staged))?;
        run_elevated(&script_path)?;
        // Повышенный скрипт уже проверяет коды возврата `sc.exe`, но состояние
        // проверяем ещё раз: `sc.exe query` прав не требует.
        verify_service_registered()
    }

    /// Останавливает и удаляет службу, затем убирает файлы.
    fn uninstall_elevated(&self) -> Result<(), InstallerError> {
        let sid = current_user_sid()?;
        let paths = InstallPaths::standard(&sid);

        let staging = Staging::new()?;
        let script_path = staging.path.join("uninstall.bat");
        write_script(&script_path, &uninstall_script(&paths))?;
        run_elevated(&script_path)
    }
}

impl HelperInstaller for WindowsInstaller {
    fn install(&self) -> Result<(), InstallerError> {
        if !cfg!(windows) {
            return Err(InstallerError::Unsupported);
        }
        self.install_elevated()
    }

    fn uninstall(&self) -> Result<(), InstallerError> {
        if !cfg!(windows) {
            return Err(InstallerError::Unsupported);
        }
        self.uninstall_elevated()
    }
}

/// Полный путь named pipe helper'а (совпадает с `pipe::pipe_path`).
pub fn pipe_path() -> String {
    format!(r"\\.\pipe\{PIPE_NAME}")
}

/// Команда создания службы: SYSTEM, автозапуск, фиксированные аргументы.
///
/// `sc.exe` требует пробел после `=`, поэтому ключи записаны как `binPath= ...`.
/// Значение `binPath` целиком берётся в кавычки с экранированием внутренних:
/// `sc.exe` разбирает командную строку на аргументы, и без внешних кавычек
/// `"C:\Program Files\..." service --pipe ...` распалось бы на несколько
/// аргументов, а `service` был бы отвергнут как неизвестный параметр.
pub fn service_create_command(paths: &InstallPaths) -> String {
    format!(
        "sc.exe create {SERVICE_NAME} binPath= {bin_path} start= auto obj= LocalSystem DisplayName= {display}",
        bin_path = quote(&service_bin_path(paths)),
        display = quote(SERVICE_DISPLAY_NAME),
    )
}

/// Команда запуска службы.
pub fn service_start_command() -> String {
    format!("sc.exe start {SERVICE_NAME}")
}

/// Команда остановки службы (ошибка игнорируется: службы может не быть).
pub fn service_stop_command() -> String {
    format!("sc.exe stop {SERVICE_NAME}")
}

/// Команда удаления службы.
pub fn service_delete_command() -> String {
    format!("sc.exe delete {SERVICE_NAME}")
}

/// Команда проверки регистрации службы; прав администратора не требует.
pub fn service_query_command() -> String {
    format!("sc.exe query {SERVICE_NAME}")
}

/// Значение `binPath`: путь к helper'у в кавычках плюс фиксированные аргументы.
///
/// Кавычки внутри пути экранируются как `\"` — иначе `sc.exe` разорвёт
/// аргумент на пробелах (`Program Files`).
pub fn service_bin_path(paths: &InstallPaths) -> String {
    format!(
        "\"{}\" service --pipe {} --runtime-dir \"{}\" --sing-box \"{}\" --allow-sid {}",
        escape_quotes(&paths.helper.to_string_lossy()),
        PIPE_NAME,
        escape_quotes(&paths.runtime_dir.to_string_lossy()),
        escape_quotes(&paths.sing_box.to_string_lossy()),
        paths.allowed_sid,
    )
}

/// Bat-скрипт установки: остановка старой службы, копирование файлов, создание и запуск.
///
/// Комментарии в bat-файле не пишем: `cmd.exe` читает его в OEM-кодировке, а
/// CRLF-скрипт с кириллицей зависит от кодовой страницы системы.
pub fn install_script(paths: &InstallPaths, staged: &StagedFiles) -> String {
    let install_dir = windows_parent(&paths.helper);
    let lines = [
        "@echo off".to_string(),
        "setlocal".to_string(),
        format!("{} >nul 2>&1", service_stop_command()),
        format!("{} >nul 2>&1", service_delete_command()),
        format!(r#"if not exist "{}" mkdir "{}""#, install_dir, install_dir),
        format!(
            r#"if not exist "{}" mkdir "{}""#,
            paths.runtime_dir.display(),
            paths.runtime_dir.display()
        ),
        // Конфиг ядра содержит адрес и ключи сервера: закрываем runtime-каталог
        // от других пользователей, оставляя чтение владельцу.
        format!(
            r#"icacls "{}" /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" "*{}:(OI)(CI)R" >nul 2>&1"#,
            paths.runtime_dir.display(),
            paths.allowed_sid,
        ),
        format!(
            r#"copy /Y "{}" "{}" >nul || exit /b 1"#,
            staged.helper.display(),
            paths.helper.display()
        ),
        format!(
            r#"copy /Y "{}" "{}" >nul || exit /b 1"#,
            staged.sing_box.display(),
            paths.sing_box.display()
        ),
        format!(
            r#"copy /Y "{}" "{}" >nul || exit /b 1"#,
            staged.wintun.display(),
            paths.wintun.display()
        ),
        service_create_command(paths),
        "if errorlevel 1 exit /b 1".to_string(),
        service_start_command(),
        "if errorlevel 1 exit /b 1".to_string(),
        "exit /b 0".to_string(),
    ];
    batch(&lines)
}

/// Bat-скрипт удаления: служба, бинари, runtime-каталог.
///
/// Как и в установочном скрипте, комментариев нет — из-за кодовой страницы.
pub fn uninstall_script(paths: &InstallPaths) -> String {
    let install_dir = windows_parent(&paths.helper);
    let lines = [
        "@echo off".to_string(),
        "setlocal".to_string(),
        format!("{} >nul 2>&1", service_stop_command()),
        format!("{} >nul 2>&1", service_delete_command()),
        format!(r#"del /F /Q "{}" >nul 2>&1"#, paths.helper.display()),
        format!(r#"del /F /Q "{}" >nul 2>&1"#, paths.sing_box.display()),
        format!(r#"del /F /Q "{}" >nul 2>&1"#, paths.wintun.display()),
        format!(r#"rmdir /S /Q "{}" >nul 2>&1"#, paths.runtime_dir.display()),
        format!(r#"rmdir "{}" >nul 2>&1"#, install_dir),
        "exit /b 0".to_string(),
    ];
    batch(&lines)
}

/// Командная строка повышения прав для отображения и логов.
pub fn elevation_command(script_path: &Path) -> String {
    format!(
        "powershell -NoProfile -Command \"{}\"",
        escape_quotes(&elevation_payload(script_path))
    )
}

/// Аргумент `-Command` для PowerShell: запуск bat-скрипта с запросом UAC.
///
/// `-Wait -PassThru` возвращает код возврата скрипта; при отказе в UAC
/// `Start-Process` бросает исключение, поэтому код 1223 (`ERROR_CANCELLED`)
/// выставляем сами — так отмена распознаётся в любой локали.
fn elevation_payload(script_path: &Path) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; \
         try {{ \
           $process = Start-Process -FilePath {} -Verb RunAs -Wait -PassThru; \
           exit $process.ExitCode \
         }} catch {{ exit {UAC_CANCELLED_CODE} }}",
        powershell_quote(&script_path.to_string_lossy()),
    )
}

/// Экранирование значения для командной строки `sc.exe` и подобных утилит.
pub fn escape_quotes(value: &str) -> String {
    value.replace('"', "\\\"")
}

/// Значение в кавычках с экранированием внутренних кавычек.
fn quote(value: &str) -> String {
    format!("\"{}\"", escape_quotes(value))
}

/// Строка в одинарных кавычках PowerShell (кавычка удваивается).
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Собирает bat-скрипт: строки CRLF, как ожидает `cmd.exe`.
fn batch(lines: &[String]) -> String {
    let mut script = lines.join("\r\n");
    script.push_str("\r\n");
    script
}

/// Каталог пути в Windows-нотации.
///
/// `Path::parent` на не-Windows не разделяет `\`, поэтому каталог отделяется
/// текстуально: так генераторы скриптов одинаковы на всех платформах.
fn windows_parent(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.rfind('\\') {
        Some(index) => text[..index].to_string(),
        None => text.to_string(),
    }
}

/// Проверка формата SID: `S-1-5-21-...`, только цифры и дефисы.
///
/// SID приходит из токена процесса, но попадает в командную строку службы,
/// поэтому формат проверяется явно.
pub fn validate_sid(sid: &str) -> Result<(), InstallerError> {
    let valid = sid.starts_with("S-1-")
        && sid.len() <= 184
        && sid
            .split('-')
            .skip(1)
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if valid {
        Ok(())
    } else {
        Err(InstallerError::Failed(
            "SID пользователя имеет неожидаемый формат".to_string(),
        ))
    }
}

/// SID пользователя, от имени которого работает приложение.
///
/// Вне Windows получить SID нельзя — установщик там всё равно не работает.
#[cfg(not(windows))]
pub fn current_user_sid() -> Result<String, InstallerError> {
    Err(InstallerError::Unsupported)
}

/// SID пользователя из токена процесса: `OpenProcessToken` + `TokenUser`.
#[cfg(windows)]
pub fn current_user_sid() -> Result<String, InstallerError> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(InstallerError::Failed(format!(
                "не удалось открыть токен процесса: {}",
                std::io::Error::last_os_error()
            )));
        }
    }

    let mut size = 0u32;
    unsafe {
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
    }
    // Буфер из u64: выравнивание под TOKEN_USER, в отличие от Vec<u8>.
    let words = (size as usize).div_ceil(std::mem::size_of::<u64>());
    let mut buffer = vec![0u64; words.max(1)];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    if ok == 0 {
        unsafe {
            CloseHandle(token);
        }
        return Err(InstallerError::Failed(format!(
            "не удалось прочитать SID пользователя: {}",
            std::io::Error::last_os_error()
        )));
    }

    let sid = unsafe { sid_to_string((*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid) };
    unsafe {
        CloseHandle(token);
    }
    sid.ok_or_else(|| InstallerError::Failed("SID не преобразован в строку".to_string()))
}

/// Преобразует бинарный SID в строку `S-1-5-...`.
#[cfg(windows)]
unsafe fn sid_to_string(sid: *mut std::ffi::c_void) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;

    let mut raw: *mut u16 = std::ptr::null_mut();
    if ConvertSidToStringSidW(sid, &mut raw) == 0 || raw.is_null() {
        return None;
    }
    let mut length = 0usize;
    while *raw.add(length) != 0 {
        length += 1;
    }
    let value = String::from_utf16_lossy(std::slice::from_raw_parts(raw, length));
    LocalFree(raw as *mut std::ffi::c_void);
    Some(value)
}

/// Запускает подготовленный скрипт от администратора: пароль спрашивает UAC.
fn run_elevated(script_path: &Path) -> Result<(), InstallerError> {
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(elevation_payload(script_path))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| InstallerError::Failed(format!("powershell: {error}")))?;
    match output.status.code() {
        Some(0) => return Ok(()),
        // Отказ в UAC: сообщение об ошибке локализовано, а код — нет.
        Some(UAC_CANCELLED_CODE) => return Err(InstallerError::RequiresPrivileges),
        _ => {}
    }
    Err(InstallerError::Failed(format!(
        "установка завершилась с ошибкой: {}",
        short_output(&output.stderr)
    )))
}

/// Проверяет, что служба зарегистрирована и запущена (`sc.exe query`).
///
/// Код возврата `sc.exe query` одинаков в любой локали, поэтому вывод не
/// разбираем: достаточно успешного завершения. Опрос не требует прав и не
/// подключается к pipe — лишних соединений не создаём.
fn verify_service_registered() -> Result<(), InstallerError> {
    let status = Command::new("sc.exe")
        .arg("query")
        .arg(SERVICE_NAME)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| InstallerError::Failed(format!("sc.exe query: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(InstallerError::Failed(
            "служба не зарегистрирована; проверьте журнал установки".to_string(),
        ))
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

fn write_script(path: &Path, contents: &str) -> Result<(), InstallerError> {
    fs::write(path, contents)
        .map_err(|error| InstallerError::Failed(format!("скрипт установки: {error}")))
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), InstallerError> {
    fs::copy(source, destination).map_err(|error| {
        InstallerError::Failed(format!(
            "не удалось скопировать {}: {error}",
            source.display()
        ))
    })?;
    Ok(())
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

fn current_exe_path() -> Result<PathBuf, InstallerError> {
    std::env::current_exe().map_err(|error| {
        InstallerError::Failed(format!("не удалось определить путь приложения: {error}"))
    })
}

/// Бинарь helper'а: env-переопределение или сосед основного исполняемого файла.
fn resolve_helper_binary() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_HELPER_BIN") {
        return require_file(path);
    }

    let executable = current_exe_path()?;
    let candidate = executable
        .parent()
        .map(|directory| directory.join("splitroute-helper.exe"));
    if let Some(path) = candidate.filter(|path| path.is_file()) {
        return Ok(path);
    }

    Err(InstallerError::MissingBinary(
        "splitroute-helper.exe рядом с приложением; соберите `cargo build -p splitroute-helper`"
            .to_string(),
    ))
}

/// Pinned sing-box: env-переопределение, sidecar приложения или dev-каталог.
fn resolve_sing_box_binary() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_SING_BOX_BIN") {
        return require_file(path);
    }

    let triple = format!("{}-pc-windows-msvc", std::env::consts::ARCH);
    let executable = current_exe_path()?;
    let directory = executable.parent().ok_or_else(|| {
        InstallerError::MissingBinary("каталог приложения не определён".to_string())
    })?;

    for name in [format!("sing-box-{triple}.exe"), "sing-box.exe".to_string()] {
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
            .join(format!("sing-box-{triple}.exe"));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(InstallerError::MissingBinary(
        "sing-box.exe; выполните ./scripts/fetch-core.sh".to_string(),
    ))
}

/// `wintun.dll` — обязательный сосед `sing-box.exe`.
///
/// Официальную библиотеку берём из `src-tauri/binaries` (см. `scripts/fetch-core.sh`),
/// её нельзя собирать из исходников: драйвер подписан.
fn resolve_wintun_library() -> Result<PathBuf, InstallerError> {
    if let Some(path) = env_path("SPLITROUTE_WINTUN_DLL") {
        return require_file(path);
    }

    let executable = current_exe_path()?;
    let directory = executable.parent().ok_or_else(|| {
        InstallerError::MissingBinary("каталог приложения не определён".to_string())
    })?;
    let candidate = directory.join("wintun.dll");
    if candidate.is_file() {
        return Ok(candidate);
    }

    if let Some(root) = directory.parent().and_then(Path::parent) {
        let candidate = root.join("src-tauri").join("binaries").join("wintun.dll");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(InstallerError::MissingBinary(
        "wintun.dll; положите официальную библиотеку в src-tauri/binaries".to_string(),
    ))
}

/// Временный каталог для файлов установки; удаляется при выходе.
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
        InstallPaths::standard("S-1-5-21-1-2-3-1001")
    }

    fn staged() -> StagedFiles {
        StagedFiles {
            helper: PathBuf::from(r"C:\Temp\splitroute-install-1\splitroute-helper.exe"),
            sing_box: PathBuf::from(r"C:\Temp\splitroute-install-1\sing-box.exe"),
            wintun: PathBuf::from(r"C:\Temp\splitroute-install-1\wintun.dll"),
        }
    }

    #[test]
    fn service_create_command_uses_system_account_and_helper_flags() {
        let command = service_create_command(&paths());
        assert!(command.starts_with("sc.exe create splitroute-helper-service"));
        assert!(command.contains("start= auto"));
        assert!(command.contains("obj= LocalSystem"));
        // sc.exe требует пробел после `=`, иначе ключ не разбирается.
        assert!(command.contains("binPath= \""));
        assert!(command.contains("start= auto"));
        assert!(command.contains("--pipe splitroute-helper"));
        assert!(command.contains("--runtime-dir"));
        assert!(command.contains("--sing-box"));
        assert!(command.contains("--allow-sid S-1-5-21-1-2-3-1001"));
        assert!(command.contains(r"C:\Program Files\SplitRoute\splitroute-helper.exe"));
        assert!(command.contains(r"C:\ProgramData\SplitRoute"));
        // Ядро: wintun.dll — файл рядом с sing-box.exe, а не аргумент службы.
        assert!(!command.contains("wintun"));
    }

    #[test]
    fn service_create_command_quotes_whole_bin_path() {
        // `sc.exe` разбирает строку на аргументы: значение binPath должно быть
        // одним токеном, иначе `service` и флаги станут «неизвестными параметрами».
        let command = service_create_command(&paths());
        let quoted = quote(&service_bin_path(&paths()));
        assert!(
            command.contains(&format!("binPath= {quoted} ")),
            "binPath должен быть одним экранированным токеном: {command}"
        );
        // Внутренние кавычки пути экранированы, а не оставлены как есть.
        assert!(quoted.starts_with("\"\\\""));
        assert!(quoted.contains("Program Files"));
    }

    #[test]
    fn service_commands_address_fixed_service_name() {
        assert_eq!(
            service_start_command(),
            "sc.exe start splitroute-helper-service"
        );
        assert_eq!(
            service_stop_command(),
            "sc.exe stop splitroute-helper-service"
        );
        assert_eq!(
            service_delete_command(),
            "sc.exe delete splitroute-helper-service"
        );
        assert_eq!(
            service_query_command(),
            "sc.exe query splitroute-helper-service"
        );
    }

    #[test]
    fn service_name_and_pipe_match_helper_contract() {
        // Значения дублируют `splitroute_helper::service::SERVICE_NAME` и
        // `splitroute_helper::pipe::pipe_path("splitroute-helper")`.
        assert_eq!(SERVICE_NAME, "splitroute-helper-service");
        assert_eq!(pipe_path(), r"\\.\pipe\splitroute-helper");
    }

    #[test]
    fn install_paths_place_wintun_next_to_sing_box() {
        let paths = paths();
        assert_eq!(paths.wintun.parent(), paths.sing_box.parent());
        // Path::parent на не-Windows не делит по `\`, поэтому сверяем каталог текстуально.
        assert_eq!(
            windows_parent(&paths.wintun),
            windows_parent(&paths.sing_box)
        );
        assert_eq!(
            windows_parent(&paths.wintun),
            r"C:\Program Files\SplitRoute"
        );
        assert_eq!(paths.runtime_dir, PathBuf::from(RUNTIME_DIR));
        assert_eq!(paths.allowed_sid, "S-1-5-21-1-2-3-1001");
    }

    #[test]
    fn elevation_command_requests_uac() {
        let command = elevation_command(Path::new(r"C:\Temp\install.bat"));
        assert!(command.contains("-Verb RunAs"));
        assert!(command.contains("-Wait"));
        assert!(command.contains("powershell"));
        assert!(command.contains(r"'C:\Temp\install.bat'"));
        // Пароль не передаётся и не хранится: только запуск через UAC.
        assert!(!command.to_lowercase().contains("password"));
    }

    #[test]
    fn elevation_payload_quotes_script_and_propagates_exit_code() {
        let payload = elevation_payload(Path::new(r"C:\Temp dir\install.bat"));
        assert!(payload.contains(r"Start-Process -FilePath 'C:\Temp dir\install.bat'"));
        assert!(payload.contains("exit $process.ExitCode"));
        // Отказ в UAC распознаём по коду, а не по локализованному тексту.
        assert!(payload.contains("catch { exit 1223 }"));
        assert!(!payload.contains("'cancel'"));
    }

    #[test]
    fn install_script_copies_binaries_and_starts_service() {
        let script = install_script(&paths(), &staged());
        assert!(script.starts_with("@echo off\r\n"));
        assert!(script.contains("\r\n"), "bat требует CRLF");
        assert!(script.contains(&format!("{} >nul 2>&1", service_stop_command())));
        assert!(script.contains(&format!("{} >nul 2>&1", service_delete_command())));
        assert!(script.contains(r#"mkdir "C:\Program Files\SplitRoute""#));
        assert!(script.contains(r#"mkdir "C:\ProgramData\SplitRoute""#));
        assert!(script.contains(r#"copy /Y "C:\Temp\splitroute-install-1\splitroute-helper.exe""#));
        assert!(script.contains(r#"copy /Y "C:\Temp\splitroute-install-1\sing-box.exe""#));
        assert!(script.contains(r#"copy /Y "C:\Temp\splitroute-install-1\wintun.dll""#));
        assert!(script.contains(r#""C:\Program Files\SplitRoute\wintun.dll""#));
        assert!(script.contains("sc.exe create splitroute-helper-service"));
        assert!(script.contains("obj= LocalSystem"));
        assert!(script.contains(&service_start_command()));
        assert!(script.contains("exit /b 0"));
        // Runtime-каталог закрыт от других пользователей.
        assert!(script.contains("icacls"));
        assert!(script.contains("*S-1-5-18:(OI)(CI)F"));
        assert!(script.contains("*S-1-5-21-1-2-3-1001:(OI)(CI)R"));
    }

    #[test]
    fn generated_bat_scripts_are_ascii() {
        // cmd.exe читает bat в OEM-кодировке: кириллица в скрипте зависела бы
        // от кодовой страницы, поэтому генерируем только ASCII.
        for (name, script) in [
            ("install", install_script(&paths(), &staged())),
            ("uninstall", uninstall_script(&paths())),
        ] {
            assert!(script.is_ascii(), "{name}.bat содержит не-ASCII символы");
        }
    }

    #[test]
    fn uninstall_script_stops_and_deletes_service() {
        let script = uninstall_script(&paths());
        assert!(script.contains("sc.exe stop splitroute-helper-service"));
        assert!(script.contains("sc.exe delete splitroute-helper-service"));
        assert!(script.contains(r#"del /F /Q "C:\Program Files\SplitRoute\splitroute-helper.exe""#));
        assert!(script.contains(r#"del /F /Q "C:\Program Files\SplitRoute\sing-box.exe""#));
        assert!(script.contains(r#"del /F /Q "C:\Program Files\SplitRoute\wintun.dll""#));
        assert!(script.contains(r#"rmdir /S /Q "C:\ProgramData\SplitRoute""#));
        assert!(script.contains(r#"rmdir "C:\Program Files\SplitRoute""#));
        // Каталог Program Files целиком не удаляется.
        assert!(!script.contains(r#"rmdir /S /Q "C:\Program Files""#));
        assert!(script.contains("exit /b 0"));
    }

    #[test]
    fn quotes_in_paths_are_escaped() {
        // В имени файла Windows кавычки невозможны, но `InstallPaths` публична:
        // проверяем, что кавычка не вырвется из-под экранирования и не разорвёт
        // значение binPath на аргументы.
        let escaped = InstallPaths {
            helper: PathBuf::from(r#"C:\Pro"gram\splitroute-helper.exe"#),
            sing_box: PathBuf::from(r#"C:\Pro"gram\sing-box.exe"#),
            wintun: PathBuf::from(r#"C:\Pro"gram\wintun.dll"#),
            runtime_dir: PathBuf::from(r#"C:\Pro"gramData"#),
            allowed_sid: "S-1-5-21-1-2-3-1001".to_string(),
        };
        let bin_path = service_bin_path(&escaped);
        assert!(
            bin_path.contains(r#"C:\Pro\"gram\splitroute-helper.exe"#),
            "кавычка в пути экранируется: {bin_path}"
        );
        assert!(
            !bin_path.contains(r#""C:\Pro"gram"#),
            "неэкранированная кавычка разорвёт binPath"
        );
        // Каждая кавычка внутри значения либо экранирована, либо закрывает токен.
        let unbalanced = bin_path
            .match_indices('"')
            .filter(|(index, _)| *index == 0 || bin_path.as_bytes()[index - 1] != b'\\')
            .count();
        assert_eq!(unbalanced % 2, 0, "кавычки должны быть парными: {bin_path}");

        // SID тоже попадает в командную строку службы: посторонние символы не пройдут.
        assert!(validate_sid(&escaped.allowed_sid).is_ok());
        let injected = InstallPaths {
            allowed_sid: "S-1-5-18 obj= LocalSystem".to_string(),
            ..paths()
        };
        assert!(validate_sid(&injected.allowed_sid).is_err());
    }

    #[test]
    fn escape_quotes_only_touches_double_quotes() {
        assert_eq!(escape_quotes(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_quotes(r"C:\Program Files"), r"C:\Program Files");
        assert_eq!(escape_quotes("'single'"), "'single'");
    }

    #[test]
    fn sid_validation_accepts_token_sids_and_rejects_garbage() {
        assert!(validate_sid("S-1-5-21-1-2-3-1001").is_ok());
        assert!(validate_sid("S-1-5-18").is_ok());
        for invalid in [
            "",
            "S-1-",
            "SYSTEM",
            "S-1-5-21-1; calc",
            r#"S-1-5-21-1"x"#,
            "S-1-5-21-1-2-3-1001 --allow-sid S-1-5-18",
        ] {
            assert!(
                validate_sid(invalid).is_err(),
                "{invalid:?} должен быть отвергнут"
            );
        }
    }

    #[test]
    fn windows_parent_splits_windows_paths() {
        assert_eq!(
            windows_parent(Path::new(r"C:\Program Files\SplitRoute\sing-box.exe")),
            r"C:\Program Files\SplitRoute"
        );
        assert_eq!(windows_parent(Path::new("file.txt")), "file.txt");
    }

    #[test]
    fn installer_is_unsupported_off_windows() {
        if cfg!(windows) {
            return;
        }
        let installer = WindowsInstaller::new();
        assert_eq!(
            installer.install().unwrap_err(),
            InstallerError::Unsupported
        );
        assert_eq!(
            installer.uninstall().unwrap_err(),
            InstallerError::Unsupported
        );
        assert_eq!(current_user_sid().unwrap_err(), InstallerError::Unsupported);
    }
}
