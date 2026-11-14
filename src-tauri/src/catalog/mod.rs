//! Каталог установленных приложений (PRD §6.3, §7).
//!
//! Сканирование только читает системные списки приложений и пользовательские
//! каталоги; привилегированных действий нет. Каждый источник разбирается чистой
//! функцией, поэтому парсинг тестируется на фикстурах без реальной ОС.

pub mod icns;
pub mod icons;
pub mod linux;
pub mod linux_icons;
pub mod macos;
pub mod windows_icons;

// Windows-разбор собирается и в тестовой конфигурации: чистые функции
// проверяются на macOS, а реестр читается только под `cfg(windows)`.
#[cfg(any(windows, test))]
pub mod windows;

use splitroute_protocol::{AppRule, InstalledApp, Platform};

/// Результат сканирования: найденные приложения и причины пропуска записей.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub apps: Vec<InstalledApp>,
    pub skipped: Vec<String>,
}

impl ScanReport {
    pub fn skip(&mut self, reason: impl Into<String>) {
        self.skipped.push(reason.into());
    }
}

/// Сканирует каталоги текущей ОС.
pub fn scan() -> ScanReport {
    let mut report = platform_scan();
    dedup_and_sort(&mut report.apps);
    report
}

#[cfg(target_os = "macos")]
fn platform_scan() -> ScanReport {
    macos::collect_from_dirs(&macos::default_dirs())
}

#[cfg(target_os = "linux")]
fn platform_scan() -> ScanReport {
    linux::collect_from_dirs(&linux::default_dirs())
}

#[cfg(target_os = "windows")]
fn platform_scan() -> ScanReport {
    windows::collect()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_scan() -> ScanReport {
    ScanReport::default()
}

/// Убирает записи с одинаковым путём и сортирует по имени без учёта регистра.
pub fn dedup_and_sort(apps: &mut Vec<InstalledApp>) {
    apps.sort_by(|left, right| left.path.cmp(&right.path));
    apps.dedup_by(|left, right| left.path == right.path);
    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
}

/// Экранирует символы, значимые в регулярном выражении sing-box. Пробелы,
/// дефисы и точки в именах файлов остаются как есть, кроме `.`.
pub fn escape_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Создаёт выключенное правило allowlist из записи каталога.
///
/// macOS: regex по каталогу `.app` — иначе helper-процессы браузера не
/// матчатся (PRD §6.3, R3). Windows/Linux: полный путь к исполняемому файлу.
pub fn rule_from_app(app: &InstalledApp) -> AppRule {
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
        Platform::Macos => {
            rule.path_regex = vec![format!("^{}/", escape_regex(&app.path))];
        }
        Platform::Windows | Platform::Linux => {
            rule.process_path = vec![app.path.clone()];
        }
    }
    rule
}

/// Каталоги текущей платформы — для тестов и вспомогательного кода.
#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, path: &str, platform: Platform) -> InstalledApp {
        InstalledApp {
            name: name.to_string(),
            path: path.to_string(),
            platform,
        }
    }

    #[test]
    fn dedup_keeps_first_path_and_sorts_by_name() {
        let mut apps = vec![
            app("zoom.us", "/Applications/zoom.us.app", Platform::Macos),
            app(
                "Google Chrome",
                "/Applications/Google Chrome.app",
                Platform::Macos,
            ),
            app(
                "Архив",
                "/Users/user/Applications/Архив.app",
                Platform::Macos,
            ),
            app(
                "google chrome",
                "/Applications/Google Chrome.app",
                Platform::Macos,
            ),
        ];
        dedup_and_sort(&mut apps);
        assert_eq!(apps.len(), 3);
        let names: Vec<&str> = apps.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, vec!["Google Chrome", "zoom.us", "Архив"]);
    }

    #[test]
    fn regex_escapes_metacharacters_only() {
        assert_eq!(
            escape_regex("/Applications/Google Chrome.app"),
            r"/Applications/Google Chrome\.app"
        );
        assert_eq!(
            escape_regex("/Applications/1Password 7 (Beta)+.app"),
            r"/Applications/1Password 7 \(Beta\)\+\.app"
        );
        assert_eq!(
            escape_regex(r"C:\Tools\a[b]{c}.exe"),
            r"C:\\Tools\\a\[b\]\{c\}\.exe"
        );
    }

    #[test]
    fn macos_rule_is_regex_and_disabled() {
        let rule = rule_from_app(&app(
            "Google Chrome",
            "/Applications/Google Chrome.app",
            Platform::Macos,
        ));
        assert!(!rule.enabled, "правило из каталога создаётся выключенным");
        assert_eq!(rule.name, "Google Chrome");
        assert_eq!(rule.platform, Platform::Macos);
        assert!(rule.process_name.is_empty());
        assert!(rule.process_path.is_empty());
        assert_eq!(
            rule.path_regex,
            vec![r"^/Applications/Google Chrome\.app/".to_string()]
        );
        assert!(!rule.id.is_empty());
    }

    #[test]
    fn linux_and_windows_rules_use_process_path() {
        for platform in [Platform::Linux, Platform::Windows] {
            let rule = rule_from_app(&app("curl", "/usr/bin/curl", platform));
            assert!(!rule.enabled);
            assert_eq!(rule.platform, platform);
            assert_eq!(rule.process_path, vec!["/usr/bin/curl".to_string()]);
            assert!(rule.path_regex.is_empty());
        }
    }

    #[test]
    fn scan_never_panics_without_catalog() {
        // Каталоги текущей ОС могут отсутствовать в песочнице: пустой отчёт
        // вместо паники.
        let report = scan();
        assert!(report.apps.iter().all(|item| !item.name.trim().is_empty()));
    }
}
