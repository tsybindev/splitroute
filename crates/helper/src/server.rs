//! Unix-транспорт helper'а: сокет `0600`, проверка uid клиента, цикл accept.
//!
//! Протокол и диспетчеризация — в [`crate::handler`]; здесь только транспорт
//! и аутентификация через `peereid`/`SO_PEERCRED`.

use std::ffi::CString;
use std::fs::{self, Permissions};
use std::io::BufReader;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use splitroute_protocol::{ErrorCode, Response};

use crate::core::HelperCore;
use crate::handler;
use crate::ipc;
use crate::platform;

/// Таймаут чтения запроса и записи ответа.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Период проверки флага завершения при неблокирующем accept.
const ACCEPT_POLL: Duration = Duration::from_millis(50);

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Параметры запуска helper'а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    pub socket_path: PathBuf,
    pub runtime_dir: PathBuf,
    pub sing_box: PathBuf,
    /// uid клиента, которому разрешено подключение.
    pub allowed_uid: u32,
}

/// Запускает цикл обслуживания до SIGTERM/SIGINT.
///
/// При выходе ядро останавливается, конфиг и сокет удаляются.
pub fn run(options: ServerOptions) -> anyhow::Result<()> {
    let core = HelperCore::new(&options.sing_box, &options.runtime_dir)
        .with_context(|| format!("не удалось подготовить {}", options.runtime_dir.display()))?;
    let shared = Arc::new(Mutex::new(core));

    let listener = bind_listener(&options.socket_path)?;
    if platform::is_root() && options.allowed_uid != 0 {
        chown_socket(&options.socket_path, options.allowed_uid)?;
    }

    install_shutdown_handler().context("не удалось установить обработчик сигналов")?;

    eprintln!(
        "splitroute-helper: слушает {} (uid {})",
        options.socket_path.display(),
        options.allowed_uid
    );

    let allowed_uid = options.allowed_uid;
    let result = accept_loop(&listener, shared.clone(), allowed_uid);

    // Новые `start` после этой точки не принимаются: иначе поток, ждавший
    // блокировку, мог бы запустить ядро уже после остановки helper'а.
    SHUTDOWN.store(true, Ordering::SeqCst);
    handler::lock(&shared).stop();
    let _ = fs::remove_file(&options.socket_path);

    result?;
    Ok(())
}

/// Создаёт сокет с правами `0600`; застарелый сокет удаляется.
fn bind_listener(path: &Path) -> anyhow::Result<UnixListener> {
    if path.exists() {
        match UnixStream::connect(path) {
            Ok(_) => anyhow::bail!("helper уже запущен: {}", path.display()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                // Никто не слушает: это застарелый сокет после падения.
                let _ = fs::remove_file(path);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("не удалось проверить сокет {}", path.display()));
            }
        }
    }

    let listener = UnixListener::bind(path)
        .with_context(|| format!("не удалось создать сокет {}", path.display()))?;
    fs::set_permissions(path, Permissions::from_mode(0o600))
        .with_context(|| format!("не удалось выставить права на {}", path.display()))?;
    listener
        .set_nonblocking(true)
        .context("не удалось перевести сокет в неблокирующий режим")?;
    Ok(listener)
}

/// Отдаёт сокет пользователю: под root иначе клиент с его uid не подключится.
fn chown_socket(path: &Path, uid: u32) -> anyhow::Result<()> {
    let path_c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| anyhow::anyhow!("путь сокета содержит NUL"))?;
    if unsafe { libc::chown(path_c.as_ptr(), uid, 0) } != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("не удалось сменить владельца {}", path.display()));
    }
    Ok(())
}

fn accept_loop(
    listener: &UnixListener,
    shared: Arc<Mutex<HelperCore>>,
    allowed_uid: u32,
) -> std::io::Result<()> {
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            return Ok(());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = shared.clone();
                thread::spawn(move || {
                    if let Err(error) = handle_connection(stream, &shared, allowed_uid) {
                        eprintln!("splitroute-helper: ошибка соединения: {error}");
                    }
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn handle_connection(
    stream: UnixStream,
    shared: &Arc<Mutex<HelperCore>>,
    allowed_uid: u32,
) -> std::io::Result<()> {
    // macOS/BSD наследует O_NONBLOCK от слушающего сокета — возвращаем
    // блокирующий режим, иначе `read_until` сразу получает WouldBlock.
    stream.set_nonblocking(false)?;
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    match peer_uid(&stream) {
        Ok(uid) if uid == allowed_uid => {}
        Ok(_) => {
            return ipc::write_response(
                &stream,
                &Response::error("", ErrorCode::Forbidden, "клиент с другим uid"),
            );
        }
        Err(_) => {
            return ipc::write_response(
                &stream,
                &Response::error(
                    "",
                    ErrorCode::Forbidden,
                    "не удалось определить uid клиента",
                ),
            );
        }
    }

    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);
    handler::serve(reader, &mut writer, shared, SHUTDOWN.load(Ordering::SeqCst))
}

/// uid клиента сокета: `getpeereid` на macOS, `SO_PEERCRED` на Linux.
#[cfg(target_os = "macos")]
fn peer_uid(stream: &UnixStream) -> std::io::Result<u32> {
    use std::os::unix::io::AsRawFd;

    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(uid)
}

/// uid клиента сокета: `getpeereid` на macOS, `SO_PEERCRED` на Linux.
#[cfg(target_os = "linux")]
fn peer_uid(stream: &UnixStream) -> std::io::Result<u32> {
    use std::os::unix::io::AsRawFd;

    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut credentials as *mut libc::ucred as *mut libc::c_void,
            &mut length,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(credentials.uid)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peer_uid(_stream: &UnixStream) -> std::io::Result<u32> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "проверка uid пира не реализована для этой платформы",
    ))
}

/// uid, которому разрешено подключаться.
///
/// Явный `--allow-uid` из LaunchDaemon/юнита имеет приоритет. Без него под root
/// используется `SUDO_UID` (dev-режим под sudo), иначе текущий uid.
pub fn resolve_allowed_uid(explicit: Option<u32>, euid: u32, sudo_uid: Option<&str>) -> u32 {
    if let Some(uid) = explicit {
        return uid;
    }
    if euid == 0 {
        return sudo_uid
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
    }
    euid
}

extern "C" fn mark_shutdown(_signal: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn install_shutdown_handler() -> std::io::Result<()> {
    let handler = mark_shutdown as *const () as libc::sighandler_t;
    unsafe {
        if libc::signal(libc::SIGTERM, handler) == libc::SIG_ERR {
            return Err(std::io::Error::last_os_error());
        }
        if libc::signal(libc::SIGINT, handler) == libc::SIG_ERR {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Используется тестами и для диагностики: процесс с таким pid существует.
pub fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_uid_wins() {
        assert_eq!(resolve_allowed_uid(Some(501), 0, Some("502")), 501);
        assert_eq!(resolve_allowed_uid(Some(501), 501, None), 501);
    }

    #[test]
    fn root_uses_sudo_uid_for_dev_mode() {
        assert_eq!(resolve_allowed_uid(None, 0, Some("501")), 501);
    }

    #[test]
    fn root_without_sudo_uid_allows_only_root() {
        assert_eq!(resolve_allowed_uid(None, 0, None), 0);
        assert_eq!(resolve_allowed_uid(None, 0, Some("not-a-uid")), 0);
    }

    #[test]
    fn regular_user_ignores_sudo_uid() {
        assert_eq!(resolve_allowed_uid(None, 501, Some("502")), 501);
        assert_eq!(resolve_allowed_uid(None, 501, None), 501);
    }
}
