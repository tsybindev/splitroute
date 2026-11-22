//! Windows: ярлыки меню «Пуск» и записи реестра `Uninstall` с `DisplayIcon`.
//!
//! Разбор ярлыка (`.lnk`) и значения `DisplayIcon` — чистые функции, поэтому
//! формат проверяется на фикстурах. Чтение реестра и обход меню «Пуск»
//! выполняются только под `cfg(windows)`.

use std::path::{Path, PathBuf};

use splitroute_protocol::{InstalledApp, Platform};

use super::ScanReport;

/// Минимальный разбор Shell Link: нужен только локальный путь цели.
///
/// Читаются `LinkFlags` заголовка, пропускается `LinkTargetIDList` и берётся
/// `LocalBasePath` + `CommonPathSuffix` из `LinkInfo`. Сетевые и относительные
/// цели без локального пути не поддерживаются — такие ярлыки пропускаются.
pub fn target_from_lnk(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 0x4c || u32::from_le_bytes(bytes[0..4].try_into().ok()?) != 0x4c {
        return None;
    }
    let flags = u32::from_le_bytes(bytes[20..24].try_into().ok()?);
    let mut offset = 0x4c;

    // HasLinkTargetIDList: u16-длина блока, затем сам блок.
    if flags & 0x1 != 0 {
        let size = u16::from_le_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
        offset = offset.checked_add(2 + size)?;
    }
    if flags & 0x2 == 0 {
        return None;
    }

    let info = bytes.get(offset..)?;
    if info.len() < 28 {
        return None;
    }
    let info_size = u32::from_le_bytes(info[0..4].try_into().ok()?) as usize;
    if info_size < 28 || info_size > info.len() {
        return None;
    }
    let header_size = u32::from_le_bytes(info[4..8].try_into().ok()?) as usize;
    let info_flags = u32::from_le_bytes(info[8..12].try_into().ok()?);
    // VolumeIDAndLocalBasePath обязателен: без него локального пути нет.
    if info_flags & 0x1 == 0 {
        return None;
    }
    let base_offset = u32::from_le_bytes(info[16..20].try_into().ok()?) as usize;
    let suffix_offset = u32::from_le_bytes(info[24..28].try_into().ok()?) as usize;

    let base = read_ansi(info, base_offset)?;
    if base.is_empty() {
        return None;
    }
    let suffix = if header_size >= 28 && suffix_offset != 0 {
        read_ansi(info, suffix_offset).unwrap_or_default()
    } else {
        String::new()
    };
    let joined = format!("{base}{suffix}");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// ANSI-строка до NUL по смещению внутри `LinkInfo`.
fn read_ansi(buffer: &[u8], offset: usize) -> Option<String> {
    let tail = buffer.get(offset..)?;
    let end = tail.iter().position(|byte| *byte == 0)?;
    let value = String::from_utf8_lossy(&tail[..end]).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Достаёт исполняемый файл из значения `DisplayIcon`.
///
/// Значение бывает `C:\App\app.exe,0`, `"C:\App\app.exe"` или путём к `.ico`.
/// Иконки и индексы отбрасываются: правилу нужен `.exe`.
pub fn path_from_display_icon(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let unquoted = trimmed.strip_prefix('"').unwrap_or(trimmed);
    let unquoted = match unquoted.rsplit_once('"') {
        Some((head, _)) if trimmed.starts_with('"') => head,
        _ => unquoted,
    };
    // Индекс иконки отделяется запятой: `app.exe,0`.
    let path = match unquoted.rsplit_once(',') {
        Some((head, index)) if index.trim().parse::<i32>().is_ok() => head,
        _ => unquoted,
    }
    .trim();
    if path.is_empty() || !path.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let expanded = expand_env(path);
    if !expanded.contains('\\') && !expanded.contains(':') {
        return None;
    }
    Some(expanded)
}

/// Раскрывает `%ProgramFiles%` и подобные переменные без обращения к системе
/// в тестах: неизвестные переменные остаются как есть.
fn expand_env(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => result.push_str(&value),
                    Err(_) => {
                        result.push('%');
                        result.push_str(name);
                        result.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                result.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    result.push_str(rest);
    result
}

/// Имя приложения из имени ярлыка или записи реестра.
pub fn name_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.trim();
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

/// Каталоги меню «Пуск»: общий и пользовательский.
pub fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (variable, tail) in [
        ("ProgramData", "Microsoft\\Windows\\Start Menu\\Programs"),
        ("APPDATA", "Microsoft\\Windows\\Start Menu\\Programs"),
    ] {
        if let Ok(base) = std::env::var(variable) {
            let path = PathBuf::from(base).join(tail);
            if !dirs.contains(&path) {
                dirs.push(path);
            }
        }
    }
    dirs
}

/// Обходит меню «Пуск» рекурсивно: ярлыки лежат в подкаталогах.
pub fn collect_from_dirs(dirs: &[PathBuf]) -> ScanReport {
    let mut report = ScanReport::default();
    for dir in dirs {
        walk(dir, &mut report);
    }
    report
}

fn walk(dir: &Path, report: &mut ScanReport) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            report.skip(format!("{}: {error}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.skip(format!("{}: {error}", dir.display()));
                continue;
            }
        };
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            walk(&path, report);
            continue;
        }
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lnk"))
        {
            continue;
        }
        let target = std::fs::read(&path)
            .ok()
            .and_then(|bytes| target_from_lnk(&bytes));
        match target {
            Some(target) => {
                let name = name_from_path(&path).unwrap_or_else(|| target.clone());
                report.apps.push(InstalledApp {
                    name,
                    path: target,
                    platform: Platform::Windows,
                });
            }
            None => report.skip(format!(
                "{}: не удалось прочитать цель ярлыка",
                path.display()
            )),
        }
    }
}

/// Сканирует меню «Пуск» и реестр `Uninstall` (HKLM, HKCU и WOW6432Node).
#[cfg(windows)]
pub fn collect() -> ScanReport {
    let mut report = collect_from_dirs(&start_menu_dirs());
    for (root, tail) in uninstall_roots() {
        for app in read_uninstall_key(root, tail) {
            report.apps.push(app);
        }
    }
    report
}

#[cfg(windows)]
fn uninstall_roots() -> Vec<(windows_sys::Win32::System::Registry::HKEY, &'static str)> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    vec![
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ]
}

/// Перечисляет подразделы `Uninstall` и берёт `DisplayIcon`.
#[cfg(windows)]
fn read_uninstall_key(
    root: windows_sys::Win32::System::Registry::HKEY,
    path: &str,
) -> Vec<InstalledApp> {
    use std::ffi::c_void;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, RRF_RT_REG_SZ,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let mut apps = Vec::new();
    let subkey = wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: указатели валидны на время вызова, строки с завершающим NUL.
    let status = unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut key) };
    if status != 0 {
        return apps;
    }

    let mut index = 0u32;
    loop {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        // SAFETY: буфер и длина согласованы, остальные указатели null допустимы.
        let status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                name.as_mut_ptr(),
                &mut name_len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            break;
        }
        index += 1;
        let value_name = wide("DisplayIcon");
        let mut buffer = [0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: `buffer` — изменяемый буфер, `size` — его размер в байтах.
        let status = unsafe {
            RegGetValueW(
                key,
                name.as_ptr(),
                value_name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast::<c_void>(),
                &mut size,
            )
        };
        if status == 0 {
            let length = (size as usize / 2).saturating_sub(1).min(buffer.len());
            let raw = String::from_utf16_lossy(&buffer[..length]);
            if let Some(path) = path_from_display_icon(&raw) {
                let name = Path::new(&path)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Приложение")
                    .to_string();
                apps.push(InstalledApp {
                    name,
                    path,
                    platform: Platform::Windows,
                });
            }
        }
    }
    // SAFETY: ключ открыт выше этой функцией и ещё не закрыт.
    unsafe { RegCloseKey(key) };
    apps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Собирает минимальный `.lnk` с `LinkInfo` и локальным путём.
    fn make_lnk(target: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x4c];
        bytes[0..4].copy_from_slice(&0x4cu32.to_le_bytes());
        bytes[20..24].copy_from_slice(&0x2u32.to_le_bytes());

        let mut info = vec![0u8; 28];
        let base = target.as_bytes();
        info.extend_from_slice(base);
        info.push(0);
        let info_size = info.len() as u32;
        info[0..4].copy_from_slice(&info_size.to_le_bytes());
        info[4..8].copy_from_slice(&28u32.to_le_bytes());
        info[8..12].copy_from_slice(&1u32.to_le_bytes());
        info[16..20].copy_from_slice(&28u32.to_le_bytes());
        info[24..28].copy_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&info);
        bytes
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "sr-catalog-windows-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lnk_target_is_extracted() {
        let path = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        assert_eq!(target_from_lnk(&make_lnk(path)).as_deref(), Some(path));
    }

    #[test]
    fn malformed_lnk_is_rejected() {
        assert!(target_from_lnk(b"short").is_none());
        assert!(target_from_lnk(&[0u8; 0x4c]).is_none());
    }

    #[test]
    fn display_icon_with_index_and_quotes() {
        assert_eq!(
            path_from_display_icon(r"C:\Tools\app.exe,0").as_deref(),
            Some(r"C:\Tools\app.exe")
        );
        assert_eq!(
            path_from_display_icon(r#""C:\Program Files\App\app.exe",0"#).as_deref(),
            Some(r"C:\Program Files\App\app.exe")
        );
        assert_eq!(
            path_from_display_icon(r"C:\Tools\app.exe").as_deref(),
            Some(r"C:\Tools\app.exe")
        );
    }

    #[test]
    fn display_icon_ignores_icons_and_relative_names() {
        assert!(path_from_display_icon(r"C:\Tools\app.ico").is_none());
        assert!(path_from_display_icon("app.exe").is_none());
        assert!(path_from_display_icon("").is_none());
    }

    #[test]
    fn collects_lnk_recursively_from_temp_dirs() {
        let dir = temp_dir("collect");
        let nested = dir.join("Google");
        std::fs::create_dir_all(&nested).unwrap();
        let lnk = make_lnk(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        std::fs::write(nested.join("Google Chrome.lnk"), lnk).unwrap();
        std::fs::write(dir.join("broken.lnk"), b"not a lnk").unwrap();

        let report = collect_from_dirs(std::slice::from_ref(&dir));
        assert_eq!(report.apps.len(), 1);
        assert_eq!(report.apps[0].name, "Google Chrome");
        assert_eq!(report.apps[0].platform, Platform::Windows);
        assert!(report
            .skipped
            .iter()
            .any(|reason| reason.contains("broken.lnk")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
