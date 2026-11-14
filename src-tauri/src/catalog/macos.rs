//! macOS: `.app`-бандлы в `/Applications`, `/System/Applications`, `~/Applications`.
//!
//! Обход — один уровень, без рекурсии: вложенные `.app` внутри других бандлов
//! относятся к конкретному приложению и в каталог не попадают.

use std::path::{Path, PathBuf};

use splitroute_protocol::{InstalledApp, Platform};

use super::ScanReport;

/// Системные каталоги приложений.
pub fn system_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ]
}

/// Пользовательский каталог приложений вместе с системными.
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = system_dirs();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Applications"));
    }
    dirs
}

/// Разбирает путь бандла. Несуществующий путь тоже даёт запись: проверка
/// доступности — отдельная задача UI (S4-T1 помечает такие правила).
pub fn app_from_bundle(path: &Path) -> Option<InstalledApp> {
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("app"))
    {
        return None;
    }
    let name = path.file_stem()?.to_str()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(InstalledApp {
        name: name.to_string(),
        path: path.to_string_lossy().into_owned(),
        platform: Platform::Macos,
    })
}

/// Читает каталоги и собирает бандлы. Ошибки чтения и посторонние файлы
/// попадают в `skipped`, сканирование продолжается.
pub fn collect_from_dirs(dirs: &[PathBuf]) -> ScanReport {
    let mut report = ScanReport::default();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                report.skip(format!("{}: {error}", dir.display()));
                continue;
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
            let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
            if !is_dir {
                continue;
            }
            match app_from_bundle(&path) {
                Some(app) => report.apps.push(app),
                None => report.skip(format!("{}: не .app", path.display())),
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "sr-catalog-macos-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bundle_path_becomes_app_with_stem_name() {
        let app = app_from_bundle(Path::new("/Applications/Google Chrome.app")).unwrap();
        assert_eq!(app.name, "Google Chrome");
        assert_eq!(app.path, "/Applications/Google Chrome.app");
        assert_eq!(app.platform, Platform::Macos);
    }

    #[test]
    fn non_bundle_paths_are_ignored() {
        assert!(app_from_bundle(Path::new("/Applications/readme.txt")).is_none());
        assert!(app_from_bundle(Path::new("/Applications/Google Chrome")).is_none());
        assert!(app_from_bundle(Path::new("/Applications/.app")).is_none());
    }

    #[test]
    fn collects_only_top_level_apps_from_temp_dirs() {
        let dir = temp_dir("collect");
        std::fs::create_dir_all(dir.join("Chrome.app/Contents/MacOS")).unwrap();
        std::fs::create_dir_all(dir.join("Nested/Inner.app")).unwrap();
        std::fs::write(dir.join("notes.txt"), "не приложение").unwrap();

        let report = collect_from_dirs(std::slice::from_ref(&dir));
        let names: Vec<&str> = report.apps.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, vec!["Chrome"]);
        assert!(
            report
                .skipped
                .iter()
                .any(|reason| reason.contains("Nested")),
            "каталог без расширения .app пропускается с причиной"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_skipped_not_fatal() {
        let dir = temp_dir("missing").join("absent");
        let report = collect_from_dirs(std::slice::from_ref(&dir));
        assert!(report.apps.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].contains("absent"));
    }

    #[test]
    fn default_dirs_include_home_and_system() {
        let dirs = default_dirs();
        assert!(dirs.iter().any(|dir| dir.ends_with("Applications")));
        assert!(dirs
            .iter()
            .any(|dir| dir == Path::new("/System/Applications")));
    }
}
