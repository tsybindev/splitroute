//! Привилегированный helper SplitRoute.
//!
//! Запускается LaunchDaemon/systemd (этап 3) или вручную в dev-режиме.
//! Принимает только фиксированный набор флагов; пути к конфигу клиент не задаёт.

use std::path::PathBuf;

#[cfg(unix)]
use splitroute_helper::{resolve_allowed_uid, run, ServerOptions};

const USAGE: &str = "\
splitroute-helper — привилегированный helper SplitRoute

Использование:
  splitroute-helper --socket <путь> --runtime-dir <каталог> --sing-box <бинарь> [--allow-uid <uid>]

Аргументы:
  --socket       Unix-сокет для IPC (абсолютный путь, права 0600)
  --runtime-dir  каталог конфига и логов ядра (создаётся helper'ом)
  --sing-box     путь к pinned sing-box
  --allow-uid    uid клиента, которому разрешено подключение
                 (по умолчанию: SUDO_UID под root, иначе текущий uid)
";

#[cfg(unix)]
fn main() {
    if let Err(message) = real_main() {
        eprintln!("splitroute-helper: {message}");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("splitroute-helper: реализация для Windows — этап 3 (S3-T4)");
    std::process::exit(1);
}

#[cfg(unix)]
fn real_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{USAGE}");
        return Ok(());
    }

    let raw = parse_args(args)?;
    if !raw.socket.is_absolute() {
        return Err("--socket должен быть абсолютным путём".to_string());
    }
    if !raw.runtime_dir.is_absolute() {
        return Err("--runtime-dir должен быть абсолютным путём".to_string());
    }
    if !raw.sing_box.is_file() {
        return Err(format!("sing-box не найден: {}", raw.sing_box.display()));
    }

    let euid = unsafe { libc::geteuid() };
    let sudo_uid = std::env::var("SUDO_UID").ok();
    let allowed_uid = resolve_allowed_uid(raw.allow_uid, euid, sudo_uid.as_deref());

    run(ServerOptions {
        socket_path: raw.socket,
        runtime_dir: raw.runtime_dir,
        sing_box: raw.sing_box,
        allowed_uid,
    })
    .map_err(|error| format!("{error:#}"))
}

#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
struct RawOptions {
    socket: PathBuf,
    runtime_dir: PathBuf,
    sing_box: PathBuf,
    allow_uid: Option<u32>,
}

#[cfg(unix)]
fn parse_args(args: Vec<String>) -> Result<RawOptions, String> {
    let mut socket = None;
    let mut runtime_dir = None;
    let mut sing_box = None;
    let mut allow_uid = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--socket" => socket = Some(PathBuf::from(next_value(&mut iter, "--socket")?)),
            "--runtime-dir" => {
                runtime_dir = Some(PathBuf::from(next_value(&mut iter, "--runtime-dir")?));
            }
            "--sing-box" => sing_box = Some(PathBuf::from(next_value(&mut iter, "--sing-box")?)),
            "--allow-uid" => {
                let value = next_value(&mut iter, "--allow-uid")?;
                allow_uid = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "--allow-uid: ожидается числовой uid".to_string())?,
                );
            }
            other => return Err(format!("неизвестный аргумент: {other}\n\n{USAGE}")),
        }
    }

    Ok(RawOptions {
        socket: socket.ok_or("не указан --socket")?,
        runtime_dir: runtime_dir.ok_or("не указан --runtime-dir")?,
        sing_box: sing_box.ok_or("не указан --sing-box")?,
        allow_uid,
    })
}

#[cfg(unix)]
fn next_value(iter: &mut std::vec::IntoIter<String>, flag: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("{flag}: не указано значение"))
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
        assert_eq!(parsed.socket, PathBuf::from("/tmp/helper.sock"));
        assert_eq!(parsed.runtime_dir, PathBuf::from("/tmp/runtime"));
        assert_eq!(parsed.sing_box, PathBuf::from("/tmp/sing-box"));
        assert_eq!(parsed.allow_uid, None);
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
    fn missing_value_is_reported() {
        let error = parse_args(args(&["--socket"])).unwrap_err();
        assert!(error.contains("--socket"));
    }

    #[test]
    fn unknown_argument_is_rejected() {
        let error = parse_args(args(&["--exec"])).unwrap_err();
        assert!(error.contains("неизвестный аргумент"));
    }
}
