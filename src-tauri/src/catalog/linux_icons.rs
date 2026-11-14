//! Linux: иконка из `Icon=` записи `.desktop` и поиск файла по XDG-каталогам.
//!
//! `Icon=` хранит либо имя темы (`firefox`), либо абсолютный путь. Имя ищется
//! в `hicolor` — тема присутствует во всех дистрибутивах и не требует разбора
//! `index.theme`; порядок размеров выбран от 64 px вниз/вверх, чтобы в список
//! попадала картинка, которую не стыдно показать.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use splitroute_protocol::InstalledApp;

use super::linux::parse_desktop_entry;

/// Размеры, в которых иконки кладут в `hicolor`: 64 — целевой, дальше запасные.
const HICOLOR_SIZES: [u32; 6] = [64, 48, 128, 256, 32, 512];

type IconMap = HashMap<String, String>;

/// Каталоги тем иконок пользователя и системы.
pub fn theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("icons"));
    }
    dirs.push(PathBuf::from("/usr/share/icons"));
    dirs
}

/// Каталоги «плоских» иконок: `/usr/share/pixmaps`.
pub fn pixmap_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/usr/share/pixmaps")]
}

/// Иконка приложения: значение `Icon=` ищется в кэше карты `.desktop`.
pub fn icon_file(app: &InstalledApp) -> Option<PathBuf> {
    let icon = icon_name_for_exec(&app.path)?;
    find_icon(&icon, &theme_dirs(), &pixmap_dirs())
}

/// Карта `Exec -> Icon`. Ключ — исполняемый файл: он же лежит в `InstalledApp`.
pub fn desktop_icon_map(dirs: &[PathBuf]) -> IconMap {
    let mut map = IconMap::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("desktop"))
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(entry) = parse_desktop_entry(&text) else {
                continue;
            };
            let Some(icon) = entry.icon else {
                continue;
            };
            map.insert(entry.exec, icon);
        }
    }
    map
}

fn icon_name_for_exec(exec: &str) -> Option<String> {
    let mut guard = match disk_map().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.is_none() {
        *guard = Some(desktop_icon_map(&super::linux::default_dirs()));
    }
    guard
        .as_ref()
        .and_then(|map| map.get(exec))
        .map(|icon| icon.to_string())
}

/// Карта строится один раз: иначе 100 иконок пересканировали бы каталоги 100 раз.
fn disk_map() -> &'static Mutex<Option<IconMap>> {
    static MAP: OnceLock<Mutex<Option<IconMap>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(None))
}

/// Ищет файл иконки: абсолютный путь используется напрямую, имя — в `hicolor`
/// нужного размера, затем в `scalable`, затем в `pixmaps`.
pub fn find_icon(icon: &str, theme_dirs: &[PathBuf], pixmap_dirs: &[PathBuf]) -> Option<PathBuf> {
    let icon = icon.trim();
    if icon.is_empty() {
        return None;
    }
    if icon.starts_with('/') {
        return supported_file(Path::new(icon));
    }
    for dir in theme_dirs {
        let hicolor = dir.join("hicolor");
        for size in HICOLOR_SIZES {
            let apps = hicolor.join(format!("{size}x{size}")).join("apps");
            for extension in ["png", "svg"] {
                let candidate = apps.join(format!("{icon}.{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        let scalable = hicolor
            .join("scalable")
            .join("apps")
            .join(format!("{icon}.svg"));
        if scalable.is_file() {
            return Some(scalable);
        }
    }
    for dir in pixmap_dirs {
        let candidate = dir.join(format!("{icon}.png"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // XPM-иконки не поддерживаются: формат требует полноценного парсера.
    None
}

/// Принимает только PNG/SVG: остальные форматы не декодируются.
fn supported_file(path: &Path) -> Option<PathBuf> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "svg") && path.is_file() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splitroute_protocol::Platform;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "sr-catalog-linux-icons-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"icon").unwrap();
    }

    fn app(path: &str) -> InstalledApp {
        InstalledApp {
            name: "app".to_string(),
            path: path.to_string(),
            platform: Platform::Linux,
        }
    }

    #[test]
    fn desktop_map_is_built_from_icon_key() {
        let dir = temp_dir("map");
        std::fs::write(
            dir.join("firefox.desktop"),
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=/usr/bin/firefox %U\nIcon=firefox\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("noicon.desktop"),
            "[Desktop Entry]\nType=Application\nName=NoIcon\nExec=/usr/bin/noicon\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("broken.desktop"),
            "[Desktop Entry]\nType=Application\nName=Broken\nExec=broken\nIcon=broken\n",
        )
        .unwrap();

        let map = desktop_icon_map(std::slice::from_ref(&dir));
        assert_eq!(
            map.get("/usr/bin/firefox").map(String::as_str),
            Some("firefox")
        );
        assert!(!map.contains_key("/usr/bin/noicon"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hicolor_png_is_preferred_and_scalable_is_used_as_fallback() {
        let theme = temp_dir("hicolor");
        touch(&theme.join("hicolor/64x64/apps/firefox.png"));
        touch(&theme.join("hicolor/scalable/apps/firefox.svg"));
        touch(&theme.join("hicolor/scalable/apps/only-svg.svg"));

        let found =
            find_icon("firefox", std::slice::from_ref(&theme), &[]).expect("PNG из hicolor");
        assert!(found.ends_with("hicolor/64x64/apps/firefox.png"));

        let scalable =
            find_icon("only-svg", std::slice::from_ref(&theme), &[]).expect("SVG из scalable");
        assert!(scalable.ends_with("hicolor/scalable/apps/only-svg.svg"));
        let _ = std::fs::remove_dir_all(&theme);
    }

    #[test]
    fn absolute_icon_path_is_used_directly() {
        let dir = temp_dir("absolute");
        let png = dir.join("app.png");
        let svg = dir.join("app.svg");
        let xpm = dir.join("app.xpm");
        touch(&png);
        touch(&svg);
        touch(&xpm);

        assert_eq!(
            find_icon(&png.to_string_lossy(), &[], &[]),
            Some(png.clone())
        );
        assert_eq!(
            find_icon(&svg.to_string_lossy(), &[], &[]),
            Some(svg.clone())
        );
        // XPM не декодируем — честный fallback.
        assert!(find_icon(&xpm.to_string_lossy(), &[], &[]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_icon_gives_none() {
        let theme = temp_dir("missing");
        assert!(find_icon("absent", std::slice::from_ref(&theme), &[]).is_none());
        assert!(find_icon("", std::slice::from_ref(&theme), &[]).is_none());
        assert!(find_icon("/nonexistent/app.png", &[], &[]).is_none());
        let _ = std::fs::remove_dir_all(&theme);
    }

    #[test]
    fn pixmap_png_is_a_last_resort() {
        let theme = temp_dir("pixmap-theme");
        let pixmaps = temp_dir("pixmaps");
        touch(&pixmaps.join("legacy.png"));
        touch(&pixmaps.join("legacy.xpm"));

        let found = find_icon(
            "legacy",
            std::slice::from_ref(&theme),
            std::slice::from_ref(&pixmaps),
        )
        .expect("PNG из pixmaps");
        assert!(found.ends_with("legacy.png"));
        let _ = std::fs::remove_dir_all(&theme);
        let _ = std::fs::remove_dir_all(&pixmaps);
    }

    #[test]
    fn icon_file_for_unknown_exec_is_none() {
        // Каталог другой ОС: карта `.desktop` не содержит такого exec.
        assert!(icon_file(&app("/nonexistent/app-for-icon-test")).is_none());
    }
}
