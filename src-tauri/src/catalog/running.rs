//! Запущенные процессы (S4-T11).
//!
//! Процессы читаются непривилегированным кодом: macOS — `proc_listpids` и
//! `proc_pidpath`, Linux — `/proc/<pid>/exe`, Windows — Toolhelp. Внешние
//! команды (`ps`, `tasklist`) не запускаются.
//!
//! Сбор и фильтрация разделены: платформенный обход отдаёт `ProcessEntry`, а
//! чистые функции фильтруют системные записи, дедуплицируют, сортируют и
//! строят правило. Поэтому правила проверяются тестами на любой ОС.

use std::collections::HashSet;

use splitroute_protocol::{AppRule, Platform, RunningApp};

use super::escape_regex;

/// Лимит списка: больше строк UI всё равно не покажет, а скан замедлится.
pub const MAX_RUNNING: usize = 500;

/// Сколько причин пропуска хранить для Debug-лога: ядро даёт сотни записей,
/// полный список раздул бы буфер логов.
pub const MAX_SKIP_NOTES: usize = 20;

/// Запись платформенного обхода: найденный процесс или причина отказа.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEntry {
    Found { pid: u32, path: String },
    Unreadable { pid: u32, reason: String },
}

/// Результат сканирования процессов.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunningReport {
    pub apps: Vec<RunningApp>,
    /// Первые `MAX_SKIP_NOTES` причин пропуска.
    pub skipped: Vec<String>,
    /// Общее число пропущенных записей (включая системные).
    pub skipped_count: usize,
    /// Системные и служебные процессы, отброшенные фильтром.
    pub system_skipped: usize,
    /// Записи, не попавшие в список из-за лимита.
    pub truncated: usize,
}

impl RunningReport {
    fn note_skip(&mut self, reason: String) {
        self.skipped_count += 1;
        if self.skipped.len() < MAX_SKIP_NOTES {
            self.skipped.push(reason);
        }
    }
}

/// Сканирует процессы текущей ОС.
pub fn scan() -> RunningReport {
    build_report(platform_collect(), current_platform(), MAX_RUNNING)
}

/// Фильтрует, дедуплицирует, сортирует и ограничивает список процессов.
pub fn build_report(entries: Vec<ProcessEntry>, platform: Platform, limit: usize) -> RunningReport {
    let mut report = RunningReport::default();
    let mut seen = HashSet::new();

    for entry in entries {
        let (pid, path) = match entry {
            ProcessEntry::Found { pid, path } => (pid, path),
            ProcessEntry::Unreadable { pid, reason } => {
                report.note_skip(format!("pid {pid}: {reason}"));
                continue;
            }
        };
        let path = path.trim();
        if path.is_empty() {
            report.note_skip(format!("pid {pid}: пустой путь процесса"));
            continue;
        }
        if is_system_path(platform, path) {
            report.system_skipped += 1;
            continue;
        }
        let Some(name) = display_name(platform, path) else {
            report.note_skip(format!("pid {pid}: не удалось определить имя"));
            continue;
        };
        // Один бинарь может быть запущен несколько раз: правило всё равно
        // строится по пути, поэтому оставляем первый pid.
        if !seen.insert(path.to_string()) {
            continue;
        }
        report.apps.push(RunningApp {
            pid,
            name,
            path: path.to_string(),
            platform,
        });
    }

    sort_apps(&mut report.apps);
    if report.apps.len() > limit {
        report.truncated = report.apps.len() - limit;
        report.apps.truncate(limit);
    }
    report
}

/// Системные и служебные процессы, которым не место в allowlist.
///
/// `/usr/bin` и `/System/Applications` не отбрасываются: CLI-утилиты и
/// системные приложения — легитимные цели правил (PRD §6.3).
pub fn is_system_path(platform: Platform, path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    match platform {
        Platform::Macos => [
            "/system/library/",
            "/library/apple/",
            "/usr/libexec/",
            "/usr/lib/",
            "/usr/sbin/",
            "/sbin/",
            "/cores/",
            "/dev/",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix)),
        Platform::Linux => [
            "/usr/lib/systemd/",
            "/usr/libexec/",
            "/usr/lib/",
            "/lib/systemd/",
            "/lib64/",
            "/lib/",
            "/sbin/",
            "/usr/sbin/",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix)),
        Platform::Windows => lower.contains("\\windows\\") || lower.starts_with("\\windows"),
    }
}

/// Каталог `.app`-бандла, если исполняемый файл лежит внутри него.
pub fn bundle_root(path: &str) -> Option<&str> {
    let lower = path.to_ascii_lowercase();
    let index = lower.find(".app/")?;
    Some(&path[..index + 4])
}

/// Имя для UI и правила: имя бандла на macOS, иначе stem исполняемого файла.
pub fn display_name(platform: Platform, path: &str) -> Option<String> {
    if platform == Platform::Macos {
        if let Some(root) = bundle_root(path) {
            return file_stem(root);
        }
    }
    file_stem(path)
}

/// Сортировка по имени без учёта регистра, затем по пути.
pub fn sort_apps(apps: &mut [RunningApp]) {
    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.path.cmp(&right.path))
    });
}

/// Создаёт выключенное правило allowlist из запущенного процесса.
///
/// macOS: путь внутри `.app` превращается в `path_regex` по каталогу бандла,
/// иначе используется `process_path` — иначе helper-процессы браузера не
/// матчатся (PRD §6.3, R3). Windows/Linux — всегда `process_path`.
pub fn rule_from_running(app: &RunningApp) -> AppRule {
    let mut rule = AppRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: app.name.clone(),
        platform: app.platform,
        process_name: Vec::new(),
        process_path: Vec::new(),
        path_regex: Vec::new(),
        enabled: false,
    };
    match app.platform {
        Platform::Macos => match bundle_root(&app.path) {
            Some(root) => rule.path_regex = vec![format!("^{}/", escape_regex(root))],
            None => rule.process_path = vec![app.path.clone()],
        },
        Platform::Windows | Platform::Linux => {
            rule.process_path = vec![app.path.clone()];
        }
    }
    rule
}

/// Имя файла без каталога; понимает оба разделителя (тесты идут на macOS).
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim()
}

/// Stem файла: расширение срезается, если после него есть имя.
fn file_stem(path: &str) -> Option<String> {
    let name = file_name(path);
    if name.is_empty() {
        return None;
    }
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    };
    if stem.trim().is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

fn current_platform() -> Platform {
    #[cfg(target_os = "macos")]
    {
        Platform::Macos
    }
    #[cfg(target_os = "linux")]
    {
        Platform::Linux
    }
    #[cfg(target_os = "windows")]
    {
        Platform::Windows
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Platform::Linux
    }
}

#[cfg(target_os = "macos")]
fn platform_collect() -> Vec<ProcessEntry> {
    let mut entries = Vec::new();
    for pid in list_pids() {
        let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: буфер валиден на время вызова, размер передаётся явно.
        let length =
            unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
        if length <= 0 {
            entries.push(ProcessEntry::Unreadable {
                pid: pid as u32,
                reason: "путь недоступен".to_string(),
            });
            continue;
        }
        entries.push(ProcessEntry::Found {
            pid: pid as u32,
            path: String::from_utf8_lossy(&buffer[..length as usize]).into_owned(),
        });
    }
    entries
}

/// Список всех pid через `libproc`.
#[cfg(target_os = "macos")]
fn list_pids() -> Vec<libc::pid_t> {
    // PROC_ALL_PIDS из libproc.h; libc константу не экспортирует.
    const PROC_ALL_PIDS: u32 = 1;
    let pid_size = std::mem::size_of::<libc::pid_t>();
    // SAFETY: нулевой указатель с нулевым размером — запрос нужного размера.
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return Vec::new();
    }
    let mut pids = vec![0 as libc::pid_t; needed as usize / pid_size + 16];
    // SAFETY: буфер и его размер в байтах согласованы.
    let bytes = unsafe {
        libc::proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * pid_size) as i32,
        )
    };
    if bytes <= 0 {
        return Vec::new();
    }
    pids.truncate(bytes as usize / pid_size);
    pids.retain(|pid| *pid > 0);
    pids
}

#[cfg(target_os = "linux")]
fn platform_collect() -> Vec<ProcessEntry> {
    let mut entries = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return entries;
    };
    for entry in dir.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        match std::fs::read_link(entry.path().join("exe")) {
            Ok(target) => {
                let path = target.to_string_lossy().into_owned();
                if path.ends_with(" (deleted)") {
                    entries.push(ProcessEntry::Unreadable {
                        pid,
                        reason: "исполняемый файл удалён".to_string(),
                    });
                } else {
                    entries.push(ProcessEntry::Found { pid, path });
                }
            }
            Err(error) => entries.push(ProcessEntry::Unreadable {
                pid,
                reason: error.to_string(),
            }),
        }
    }
    entries
}

#[cfg(windows)]
fn platform_collect() -> Vec<ProcessEntry> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: снимок создаётся и закрывается в этой функции; указатели
    // передаются на валидные структуры с согласованными размерами.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut entries = Vec::new();
        let mut process = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut has_next = Process32FirstW(snapshot, &mut process) != 0;
        while has_next {
            let pid = process.th32ProcessID;
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                entries.push(ProcessEntry::Unreadable {
                    pid,
                    reason: "нет доступа к процессу".to_string(),
                });
            } else {
                let mut buffer = [0u16; 32768];
                let mut size = buffer.len() as u32;
                let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) != 0;
                CloseHandle(handle);
                if ok {
                    entries.push(ProcessEntry::Found {
                        pid,
                        path: String::from_utf16_lossy(&buffer[..size as usize]),
                    });
                } else {
                    entries.push(ProcessEntry::Unreadable {
                        pid,
                        reason: "путь недоступен".to_string(),
                    });
                }
            }
            has_next = Process32NextW(snapshot, &mut process) != 0;
        }
        CloseHandle(snapshot);
        entries
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_collect() -> Vec<ProcessEntry> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(pid: u32, path: &str) -> ProcessEntry {
        ProcessEntry::Found {
            pid,
            path: path.to_string(),
        }
    }

    fn app(name: &str, path: &str, platform: Platform) -> RunningApp {
        RunningApp {
            pid: 100,
            name: name.to_string(),
            path: path.to_string(),
            platform,
        }
    }

    #[test]
    fn system_paths_are_filtered_per_platform() {
        for path in [
            "/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow",
            "/usr/libexec/remoted",
            "/usr/sbin/mDNSResponder",
        ] {
            assert!(is_system_path(Platform::Macos, path), "{path}");
        }
        // Системные приложения и CLI-утилиты — легитимные цели правил.
        assert!(!is_system_path(
            Platform::Macos,
            "/System/Applications/Safari.app/Contents/MacOS/Safari"
        ));
        assert!(!is_system_path(Platform::Macos, "/usr/bin/curl"));

        for path in [
            "/usr/lib/systemd/systemd",
            "/usr/libexec/dbus-daemon",
            "/sbin/init",
        ] {
            assert!(is_system_path(Platform::Linux, path), "{path}");
        }
        assert!(!is_system_path(Platform::Linux, "/usr/bin/firefox"));

        assert!(is_system_path(
            Platform::Windows,
            "C:\\Windows\\System32\\svchost.exe"
        ));
        assert!(!is_system_path(
            Platform::Windows,
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
        ));
    }

    #[test]
    fn system_entries_are_counted_and_skipped() {
        let entries = vec![
            found(1, "/usr/libexec/foo"),
            found(2, "/Applications/Telegram.app/Contents/MacOS/Telegram"),
            found(3, "/System/Library/CoreServices/bar"),
        ];
        let report = build_report(entries, Platform::Macos, MAX_RUNNING);
        assert_eq!(report.apps.len(), 1);
        assert_eq!(report.apps[0].name, "Telegram");
        assert_eq!(report.system_skipped, 2);
    }

    #[test]
    fn unreadable_entries_are_reported_with_pid() {
        let entries = vec![ProcessEntry::Unreadable {
            pid: 77,
            reason: "путь недоступен".to_string(),
        }];
        let report = build_report(entries, Platform::Macos, MAX_RUNNING);
        assert!(report.apps.is_empty());
        assert_eq!(report.skipped_count, 1);
        assert_eq!(report.skipped, vec!["pid 77: путь недоступен"]);
    }

    #[test]
    fn skip_notes_are_capped() {
        let entries = (0..(MAX_SKIP_NOTES + 30))
            .map(|pid| ProcessEntry::Unreadable {
                pid: pid as u32,
                reason: "нет пути".to_string(),
            })
            .collect();
        let report = build_report(entries, Platform::Macos, MAX_RUNNING);
        assert_eq!(report.skipped.len(), MAX_SKIP_NOTES);
        assert_eq!(report.skipped_count, MAX_SKIP_NOTES + 30);
    }

    #[test]
    fn duplicate_paths_keep_first_pid() {
        let entries = vec![
            found(10, "/Applications/Telegram.app/Contents/MacOS/Telegram"),
            found(11, "/Applications/Telegram.app/Contents/MacOS/Telegram"),
        ];
        let report = build_report(entries, Platform::Macos, MAX_RUNNING);
        assert_eq!(report.apps.len(), 1);
        assert_eq!(report.apps[0].pid, 10);
    }

    #[test]
    fn apps_are_sorted_by_name_case_insensitively() {
        let entries = vec![
            found(1, "/Applications/zoom.us.app/Contents/MacOS/zoom.us"),
            found(
                2,
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            ),
            found(3, "/Applications/arc.app/Contents/MacOS/arc"),
        ];
        let report = build_report(entries, Platform::Macos, MAX_RUNNING);
        let names: Vec<&str> = report.apps.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, vec!["arc", "Google Chrome", "zoom.us"]);
    }

    #[test]
    fn limit_truncates_and_reports_remainder() {
        let entries = (1..=5)
            .map(|index| {
                found(
                    index,
                    &format!("/Applications/App{index}.app/Contents/MacOS/App{index}"),
                )
            })
            .collect();
        let report = build_report(entries, Platform::Macos, 3);
        assert_eq!(report.apps.len(), 3);
        assert_eq!(report.truncated, 2);
    }

    #[test]
    fn display_name_prefers_bundle_name() {
        assert_eq!(
            display_name(
                Platform::Macos,
                "/Applications/Google Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"
            )
            .as_deref(),
            Some("Google Chrome"),
            "имя берётся из внешнего .app, а не из вложенного helper'а"
        );
        assert_eq!(
            display_name(
                Platform::Macos,
                "/Applications/zoom.us.app/Contents/MacOS/zoom.us"
            )
            .as_deref(),
            Some("zoom.us")
        );
        assert_eq!(
            display_name(Platform::Macos, "/usr/bin/curl").as_deref(),
            Some("curl")
        );
        assert_eq!(
            display_name(Platform::Windows, "C:\\Tools\\app.exe").as_deref(),
            Some("app")
        );
        assert!(display_name(Platform::Linux, "   ").is_none());
    }

    #[test]
    fn bundle_root_is_case_insensitive() {
        assert_eq!(
            bundle_root("/Applications/Foo.APP/Contents/MacOS/Foo"),
            Some("/Applications/Foo.APP")
        );
        assert!(bundle_root("/usr/bin/curl").is_none());
    }

    #[test]
    fn rule_from_bundle_path_uses_disabled_regex() {
        let rule = rule_from_running(&app(
            "Google Chrome",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            Platform::Macos,
        ));
        assert!(
            !rule.enabled,
            "правило из запущенного создаётся выключенным"
        );
        assert_eq!(rule.name, "Google Chrome");
        assert!(rule.process_path.is_empty());
        assert_eq!(
            rule.path_regex,
            vec![r"^/Applications/Google Chrome\.app/".to_string()]
        );
        assert!(!rule.id.is_empty());
    }

    #[test]
    fn rule_from_plain_macos_binary_uses_process_path() {
        let rule = rule_from_running(&app("curl", "/usr/bin/curl", Platform::Macos));
        assert!(!rule.enabled);
        assert_eq!(rule.process_path, vec!["/usr/bin/curl".to_string()]);
        assert!(rule.path_regex.is_empty());
    }

    #[test]
    fn rule_from_windows_and_linux_uses_process_path() {
        for (platform, path) in [
            (Platform::Linux, "/usr/bin/firefox"),
            (
                Platform::Windows,
                "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            ),
        ] {
            let rule = rule_from_running(&app("app", path, platform));
            assert!(!rule.enabled);
            assert_eq!(rule.platform, platform);
            assert_eq!(rule.process_path, vec![path.to_string()]);
            assert!(rule.path_regex.is_empty());
        }
    }

    #[test]
    fn real_scan_returns_sane_entries() {
        // Мягкие проверки: конкретные процессы и их число не фиксируются.
        let report = scan();
        assert!(report.apps.len() <= MAX_RUNNING);
        for app in &report.apps {
            assert!(app.pid > 0);
            assert!(!app.name.trim().is_empty(), "{app:?}");
            assert!(!app.path.trim().is_empty(), "{app:?}");
            assert_eq!(app.platform, current_platform());
        }
    }
}
