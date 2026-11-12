//! Разбор аргументов helper'а.
//!
//! Набор флагов фиксирован: клиент не может передать произвольные пути или
//! команды. Unix- и Windows-варианты различаются только транспортом и
//! способом проверки клиента.

use std::path::PathBuf;

pub const USAGE: &str = "\
splitroute-helper — привилегированный helper SplitRoute

Использование:
  splitroute-helper --socket <путь> --runtime-dir <каталог> --sing-box <бинарь> [--allow-uid <uid>]
  splitroute-helper service --pipe <имя> --runtime-dir <каталог> --sing-box <бинарь> --allow-sid <sid>
  splitroute-helper console --pipe <имя> --runtime-dir <каталог> --sing-box <бинарь> --allow-sid <sid>

Аргументы:
  --socket       Unix-сокет для IPC (абсолютный путь, права 0600)
  --pipe         имя named pipe Windows (без \\\\.\\pipe\\)
  --runtime-dir  каталог конфига и логов ядра (создаётся helper'ом)
  --sing-box     путь к pinned sing-box
  --allow-uid    uid клиента, которому разрешено подключение (Unix)
  --allow-sid    SID пользователя, которому разрешено подключение (Windows)

Подкоманды (Windows):
  service        работа под диспетчером служб (ставится установщиком)
  console        запуск в консоли от администратора, без регистрации службы
";

/// Разобранные аргументы. Неиспользованные для платформы поля остаются `None`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RawOptions {
    pub socket: Option<PathBuf>,
    pub pipe: Option<String>,
    pub runtime_dir: Option<PathBuf>,
    pub sing_box: Option<PathBuf>,
    pub allow_uid: Option<u32>,
    pub allow_sid: Option<String>,
}

pub fn parse_args(args: Vec<String>) -> Result<RawOptions, String> {
    let mut raw = RawOptions::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--socket" => raw.socket = Some(PathBuf::from(next_value(&mut iter, "--socket")?)),
            "--pipe" => raw.pipe = Some(next_value(&mut iter, "--pipe")?),
            "--runtime-dir" => {
                raw.runtime_dir = Some(PathBuf::from(next_value(&mut iter, "--runtime-dir")?));
            }
            "--sing-box" => {
                raw.sing_box = Some(PathBuf::from(next_value(&mut iter, "--sing-box")?));
            }
            "--allow-uid" => {
                let value = next_value(&mut iter, "--allow-uid")?;
                raw.allow_uid = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "--allow-uid: ожидается числовой uid".to_string())?,
                );
            }
            "--allow-sid" => raw.allow_sid = Some(next_value(&mut iter, "--allow-sid")?),
            other => return Err(format!("неизвестный аргумент: {other}\n\n{USAGE}")),
        }
    }
    Ok(raw)
}

fn next_value(iter: &mut std::vec::IntoIter<String>, flag: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("{flag}: не указано значение"))
}

/// Подкоманда Windows-helper'а.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subcommand {
    /// Работа под диспетчером служб.
    Service,
    /// Запуск в консоли, без регистрации службы.
    Console,
}

/// Отделяет подкоманду Windows-helper'а от остальных аргументов.
///
/// Разбор вынесен сюда, а не в `service`, чтобы тесты выполнялись и на macOS:
/// подкоманды — часть контракта командной строки, а не привилегий.
pub fn split_subcommand(args: Vec<String>) -> Result<(Subcommand, Vec<String>), String> {
    let mut iter = args.into_iter();
    let Some(first) = iter.next() else {
        return Err(format!("не указана подкоманда\n\n{USAGE}"));
    };
    let rest: Vec<String> = iter.collect();
    match first.as_str() {
        "service" => Ok((Subcommand::Service, rest)),
        "console" => Ok((Subcommand::Console, rest)),
        other => Err(format!("неизвестная подкоманда: {other}\n\n{USAGE}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_required_arguments() {
        let parsed = parse_args(args(&[
            "--socket",
            "/tmp/helper.sock",
            "--runtime-dir",
            "/tmp/runtime",
            "--sing-box",
            "/tmp/sing-box",
        ]))
        .unwrap();
        assert_eq!(parsed.socket, Some(PathBuf::from("/tmp/helper.sock")));
        assert_eq!(parsed.runtime_dir, Some(PathBuf::from("/tmp/runtime")));
        assert_eq!(parsed.sing_box, Some(PathBuf::from("/tmp/sing-box")));
        assert_eq!(parsed.allow_uid, None);
        assert_eq!(parsed.pipe, None);
    }

    #[test]
    fn parses_allow_uid() {
        let parsed = parse_args(args(&[
            "--socket",
            "/tmp/helper.sock",
            "--runtime-dir",
            "/tmp/runtime",
            "--sing-box",
            "/tmp/sing-box",
            "--allow-uid",
            "501",
        ]))
        .unwrap();
        assert_eq!(parsed.allow_uid, Some(501));
    }

    #[test]
    fn parses_windows_pipe_arguments() {
        let parsed = parse_args(args(&[
            "--pipe",
            "splitroute-helper",
            "--runtime-dir",
            r"C:\ProgramData\SplitRoute",
            "--sing-box",
            r"C:\ProgramData\SplitRoute\sing-box.exe",
            "--allow-sid",
            "S-1-5-21-1-2-3-1001",
        ]))
        .unwrap();
        assert_eq!(parsed.pipe.as_deref(), Some("splitroute-helper"));
        assert_eq!(parsed.allow_sid.as_deref(), Some("S-1-5-21-1-2-3-1001"));
        assert_eq!(parsed.allow_uid, None);
    }

    #[test]
    fn missing_value_is_reported() {
        let error = parse_args(args(&["--socket"])).unwrap_err();
        assert!(error.contains("--socket"));
    }

    #[test]
    fn unknown_argument_is_rejected() {
        let error = parse_args(args(&["--exec"])).unwrap_err();
        assert!(error.contains("неизвестный аргумент"));
    }

    #[test]
    fn splits_service_subcommand() {
        let (subcommand, rest) = split_subcommand(args(&["service", "--pipe", "p"])).unwrap();
        assert_eq!(subcommand, Subcommand::Service);
        assert_eq!(rest, args(&["--pipe", "p"]));
    }

    #[test]
    fn splits_console_subcommand() {
        let (subcommand, rest) = split_subcommand(args(&["console", "--pipe", "p"])).unwrap();
        assert_eq!(subcommand, Subcommand::Console);
        assert_eq!(rest, args(&["--pipe", "p"]));
    }

    #[test]
    fn missing_subcommand_is_rejected() {
        let error = split_subcommand(Vec::new()).unwrap_err();
        assert!(error.contains("подкоманда"));
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let error = split_subcommand(args(&["install"])).unwrap_err();
        assert!(error.contains("неизвестная подкоманда"));
    }

    #[test]
    fn console_is_documented_in_usage() {
        assert!(USAGE.contains("console"), "console должен быть в USAGE");
        assert!(USAGE.contains("service"));
    }
}
