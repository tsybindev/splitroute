//! Linux: `.desktop`-файлы из `/usr/share/applications` и
//! `~/.local/share/applications`.
//!
//! Путь берётся из `Exec` (первый токен), имя — из `Name`. Записи без
//! абсолютного исполняемого пути, скрытые и не-`Application` пропускаются с
//! причиной: правило по такому пути sing-box не сматчит.

use std::path::{Path, PathBuf};

use splitroute_protocol::{InstalledApp, Platform};

use super::ScanReport;

/// Разобранная запись `.desktop`, нужная каталогу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
}

pub fn system_dir() -> PathBuf {
    PathBuf::from("/usr/share/applications")
}

/// Пользовательский каталог вместе с системным.
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![system_dir()];
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("applications"));
    }
    dirs
}

/// Разбирает содержимое `.desktop`. Читается только группа `[Desktop Entry]`.
pub fn parse_desktop_entry(text: &str) -> Option<DesktopEntry> {
    let mut in_group = false;
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut hidden = false;
    let mut no_display = false;
    let mut entry_type: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            // Локализованные ключи (Name[ru]) не заменяют базовый Name.
            "Name" => name = Some(value.to_string()),
            "Exec" => exec = Some(value.to_string()),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            "Type" => entry_type = Some(value.to_string()),
            _ => {}
        }
    }

    if hidden || no_display {
        return None;
    }
    if entry_type
        .as_deref()
        .is_some_and(|kind| kind != "Application")
    {
        return None;
    }
    let name = name?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let exec = executable_from_exec(&exec?)?;
    Some(DesktopEntry { name, exec })
}

/// Первый токен `Exec` без field-кодов `%f`/`%U`, кавычек и обёртки `env`.
/// Возвращает только абсолютный путь: относительные команды не годятся для
/// правила по процессу.
pub fn executable_from_exec(value: &str) -> Option<String> {
    let tokens = split_exec(value);
    let mut index = 0;
    // Exec=env VAR=1 /usr/bin/app — обёртка, сам бинарь дальше.
    if tokens.first().is_some_and(|token| token == "env") {
        index = 1;
        while tokens
            .get(index)
            .is_some_and(|token| token.contains('=') && !token.starts_with('/'))
        {
            index += 1;
        }
    }
    let command = tokens.get(index)?;
    let command = command.trim();
    if !command.starts_with('/') {
        return None;
    }
    Some(command.to_string())
}

/// Разбивает `Exec` по пробелам с учётом кавычек; field-коды отбрасываются.
fn split_exec(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' => match quote {
                Some(open) if open == character => quote = None,
                Some(_) => current.push(character),
                None => quote = Some(character),
            },
            ' ' | '\t' if quote.is_none() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
        .into_iter()
        .filter(|token| !token.starts_with('%'))
        .collect()
}

/// Читает каталоги `.desktop`. Битые файлы не роняют скан: причина в `skipped`.
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
            if !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("desktop"))
            {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    report.skip(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            match parse_desktop_entry(&text) {
                // .desktop-файл часто остаётся после удаления программы: путь
                // из Exec может уже не существовать.
                Some(entry) if !Path::new(&entry.exec).is_file() => report.skip(format!(
                    "{}: Exec указывает на отсутствующий файл {}",
                    path.display(),
                    entry.exec
                )),
                Some(entry) => report.apps.push(InstalledApp {
                    name: entry.name,
                    path: entry.exec,
                    platform: Platform::Linux,
                }),
                None => report.skip(format!(
                    "{}: нет приложения с абсолютным Exec",
                    path.display()
                )),
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
            "sr-catalog-linux-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(name: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/{name} %U\n")
    }

    #[test]
    fn parses_name_and_exec_without_argument() {
        let parsed = parse_desktop_entry(&entry("firefox")).unwrap();
        assert_eq!(parsed.name, "firefox");
        assert_eq!(parsed.exec, "/usr/bin/firefox");
    }

    #[test]
    fn localized_name_does_not_replace_base_name() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[ru]=Файлы\nName[de]=Dateien\nExec=/usr/bin/nautilus --new-window\n";
        let parsed = parse_desktop_entry(text).unwrap();
        assert_eq!(parsed.name, "Files");
        assert_eq!(parsed.exec, "/usr/bin/nautilus");
    }

    #[test]
    fn quoted_exec_with_spaces_is_handled() {
        let text = "[Desktop Entry]\nType=Application\nName=Chrome\nExec=\"/opt/google/chrome/chrome\" --flag\n";
        let parsed = parse_desktop_entry(text).unwrap();
        assert_eq!(parsed.exec, "/opt/google/chrome/chrome");
    }

    #[test]
    fn env_wrapper_is_unwrapped() {
        let text =
            "[Desktop Entry]\nType=Application\nName=App\nExec=env GTK_THEME=Dark /usr/bin/app\n";
        assert_eq!(parse_desktop_entry(text).unwrap().exec, "/usr/bin/app");
    }

    #[test]
    fn relative_command_is_rejected() {
        let text = "[Desktop Entry]\nType=Application\nName=App\nExec=app %f\n";
        assert!(parse_desktop_entry(text).is_none());
    }

    #[test]
    fn hidden_and_non_application_entries_are_rejected() {
        let hidden =
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=/usr/bin/x\nHidden=true\n";
        assert!(parse_desktop_entry(hidden).is_none());

        let link = "[Desktop Entry]\nType=Link\nName=Site\nURL=https://example.invalid\n";
        assert!(parse_desktop_entry(link).is_none());

        let other_group = "[Desktop Action new]\nName=New\nExec=/usr/bin/x\n";
        assert!(parse_desktop_entry(other_group).is_none());
    }

    #[test]
    fn field_codes_are_dropped() {
        assert_eq!(
            executable_from_exec("/usr/bin/app %U %F --flag").as_deref(),
            Some("/usr/bin/app")
        );
    }

    #[test]
    fn collect_skips_broken_files_and_keeps_valid_ones() {
        let dir = temp_dir("collect");
        // Реальный исполняемый файл: каталог проверяет, что путь существует.
        let binary = dir.join("app");
        std::fs::write(&binary, "#!/bin/sh\n").unwrap();
        std::fs::write(
            dir.join("good.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=app\nExec={} %U\n",
                binary.display()
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("broken.desktop"),
            "[Desktop Entry]\nType=Application\nName=Broken\nExec=broken\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("stale.desktop"),
            "[Desktop Entry]\nType=Application\nName=Stale\nExec=/nonexistent/app\n",
        )
        .unwrap();
        std::fs::write(dir.join("not-a-desktop.txt"), "мусор").unwrap();

        let report = collect_from_dirs(std::slice::from_ref(&dir));
        assert_eq!(report.apps.len(), 1);
        assert_eq!(report.apps[0].name, "app");
        assert_eq!(report.apps[0].platform, Platform::Linux);
        assert!(report
            .skipped
            .iter()
            .any(|reason| reason.contains("broken.desktop")));
        assert!(
            report
                .skipped
                .iter()
                .any(|reason| reason.contains("отсутствующий файл")),
            "запись с несуществующим Exec пропускается с причиной"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_skipped_not_fatal() {
        let dir = temp_dir("missing").join("absent");
        let report = collect_from_dirs(std::slice::from_ref(&dir));
        assert!(report.apps.is_empty());
        assert_eq!(report.skipped.len(), 1);
    }
}
