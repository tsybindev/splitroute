//! Привилегированный helper SplitRoute.
//!
//! Запускается systemd/LaunchDaemon (Unix) или как служба Windows под SYSTEM.
//! Принимает только фиксированный набор флагов; пути клиент задать не может.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{}", splitroute_helper::args::USAGE);
        return;
    }
    if let Err(message) = real_main(args) {
        eprintln!("splitroute-helper: {message}");
        std::process::exit(1);
    }
}

#[cfg(unix)]
fn real_main(args: Vec<String>) -> Result<(), String> {
    use splitroute_helper::args::parse_args;
    use splitroute_helper::resolve_allowed_uid;

    let raw = parse_args(args)?;
    let socket = raw.socket.ok_or("не указан --socket")?;
    let runtime_dir = raw.runtime_dir.ok_or("не указан --runtime-dir")?;
    let sing_box = raw.sing_box.ok_or("не указан --sing-box")?;

    if !socket.is_absolute() {
        return Err("--socket должен быть абсолютным путём".to_string());
    }
    if !runtime_dir.is_absolute() {
        return Err("--runtime-dir должен быть абсолютным путём".to_string());
    }
    if !sing_box.is_file() {
        return Err(format!("sing-box не найден: {}", sing_box.display()));
    }

    let euid = unsafe { libc::geteuid() };
    let sudo_uid = std::env::var("SUDO_UID").ok();
    let allowed_uid = resolve_allowed_uid(raw.allow_uid, euid, sudo_uid.as_deref());

    splitroute_helper::run(splitroute_helper::ServerOptions {
        socket_path: socket,
        runtime_dir,
        sing_box,
        allowed_uid,
    })
    .map_err(|error| format!("{error:#}"))
}

#[cfg(windows)]
fn real_main(args: Vec<String>) -> Result<(), String> {
    splitroute_helper::service::dispatch(args)
}
