//! Windows-транспорт helper'а: named pipe с ACL на SID пользователя.
//!
//! Pipe создаётся с дискреционным ACL из `--allow-sid`: доступ получают этот
//! пользователь, SYSTEM и Administrators. Права проверяет ядро ОС при
//! подключении, а helper дополнительно сверяет SID процесса-клиента — не
//! полагаемся только на ACL.
//!
//! Протокол и диспетчеризация — в [`crate::handler`], один JSON на строку.
//!
//! Ожидание клиента блокирующее, поэтому оно живёт в отдельном потоке, а
//! главный поток ждёт подключения с таймаутом и проверяет флаг остановки.
//! Так `SERVICE_CONTROL_STOP` не повисает на `ConnectNamedPipe`: `run`
//! возвращается, служба завершается, процесс закрывает pipe.

use std::ffi::c_void;
use std::io::BufReader;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use splitroute_protocol::{ErrorCode, Response};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER,
    },
    Storage::FileSystem::{FlushFileBuffers, PIPE_ACCESS_DUPLEX},
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    },
    System::Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION},
};

use crate::core::HelperCore;
use crate::handler;
use crate::ipc;

/// Клиент успел подключиться до вызова `ConnectNamedPipe`.
const ERROR_PIPE_CONNECTED: i32 = 535;
/// Период проверки флага остановки, пока клиента нет.
const ACCEPT_POLL: Duration = Duration::from_millis(100);

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Параметры запуска helper'а под Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    /// Имя pipe без префикса, например `splitroute-helper`.
    pub pipe_name: String,
    pub runtime_dir: PathBuf,
    pub sing_box: PathBuf,
    /// SID пользователя, которому разрешено подключение (`S-1-5-21-...`).
    pub allowed_sid: String,
}

/// Запускает цикл обслуживания до [`request_shutdown`].
///
/// При выходе ядро останавливается, конфиг удаляется.
pub fn run(options: ServerOptions) -> anyhow::Result<()> {
    let core = HelperCore::new(&options.sing_box, &options.runtime_dir)
        .with_context(|| format!("не удалось подготовить {}", options.runtime_dir.display()))?;
    let shared = Arc::new(Mutex::new(core));

    // ACL проверяется при создании первого экземпляра pipe в потоке ожидания;
    // здесь только убеждаемся, что SDDL разбирается, и падаем рано.
    SecurityDescriptor::from_sddl(&pipe_sddl(&options.allowed_sid))
        .context("не удалось построить ACL для named pipe")?;

    if let Err(error) = crate::killswitch::cleanup_stale(&options.runtime_dir) {
        eprintln!("splitroute-helper: не удалось снять старые правила kill switch: {error}");
    }

    eprintln!(
        "splitroute-helper: слушает {} (sid {})",
        pipe_path(&options.pipe_name),
        options.allowed_sid
    );

    let result = accept_loop(&options, shared.clone());

    SHUTDOWN.store(true, Ordering::SeqCst);
    handler::lock(&shared).stop();

    result
}

/// Запрашивает остановку helper'а (вызывается обработчиком службы).
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn accept_loop(options: &ServerOptions, shared: Arc<Mutex<HelperCore>>) -> anyhow::Result<()> {
    // Поток ожидания клиента: блокирующий ConnectNamedPipe нельзя прервать
    // извне, поэтому он живёт отдельно и передаёт готовый handle.
    let (sender, receiver) = mpsc::channel();
    let name = pipe_path(&options.pipe_name);
    let sddl = pipe_sddl(&options.allowed_sid);
    thread::spawn(move || {
        // Дескриптор строится в этом потоке: SECURITY_DESCRIPTOR — не Send.
        let Ok(descriptor) = SecurityDescriptor::from_sddl(&sddl) else {
            return;
        };
        let wide_name = to_wide(&name);
        while !SHUTDOWN.load(Ordering::SeqCst) {
            let Ok(handle) = create_instance(&wide_name, &descriptor) else {
                thread::sleep(ACCEPT_POLL);
                continue;
            };
            if !wait_for_client(handle) {
                continue;
            }
            if sender.send(PipeHandle(handle)).is_err() {
                return;
            }
        }
    });

    let allowed_sid = options.allowed_sid.clone();
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            return Ok(());
        }
        match receiver.recv_timeout(ACCEPT_POLL) {
            Ok(handle) => {
                let shared = shared.clone();
                let allowed_sid = allowed_sid.clone();
                thread::spawn(move || {
                    if let Err(error) = handle_connection(handle, &shared, &allowed_sid) {
                        eprintln!("splitroute-helper: ошибка соединения: {error}");
                    }
                });
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow::anyhow!("поток ожидания pipe завершился"));
            }
        }
    }
}

/// Владелец handle, который можно передать в другой поток.
struct PipeHandle(HANDLE);

// HANDLE — это указатель на объект ядра; владение передаётся ровно один раз.
unsafe impl Send for PipeHandle {}

/// Создаёт экземпляр pipe с заданным ACL.
fn create_instance(name: &[u16], descriptor: &SecurityDescriptor) -> anyhow::Result<HANDLE> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.as_ptr(),
        bInheritHandle: 0,
    };
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error()).context("не удалось создать named pipe");
    }
    Ok(handle)
}

/// Блокирующе ждёт клиента; `false` — неудача, пробуем новый экземпляр.
fn wait_for_client(handle: HANDLE) -> bool {
    if unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0 {
        return true;
    }
    if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED) {
        return true;
    }
    unsafe {
        CloseHandle(handle);
    }
    false
}

fn handle_connection(
    handle: PipeHandle,
    shared: &Arc<Mutex<HelperCore>>,
    allowed_sid: &str,
) -> anyhow::Result<()> {
    let handle = handle.0;
    // SID проверяем до передачи владения: функция только читает сведения о клиенте.
    if client_process_sid(handle).as_deref() != Some(allowed_sid) {
        let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut writer = std::fs::File::from(owned);
        ipc::write_response(
            &mut writer,
            &Response::error("", ErrorCode::Forbidden, "клиент с другим SID"),
        )?;
        // Ответ уже в буфере ядра: без сброса закрытие handle может выкинуть
        // незачитанные клиентом данные (та же гонка, что ниже).
        unsafe {
            FlushFileBuffers(handle);
        }
        return Ok(());
    }

    // Владение handle переходит к OwnedHandle: закрытие при выходе из функции.
    let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    let file = std::fs::File::from(owned);
    let reader = BufReader::new(file.try_clone()?);
    let mut writer = file;
    handler::serve(reader, &mut writer, shared, SHUTDOWN.load(Ordering::SeqCst))?;

    // Ответ записан в буфер ядра, но клиент мог ещё не прочитать его:
    // DisconnectNamedPipe выбрасывает незачитанные данные, и клиент видит
    // EOF («пустой ответ»). FlushFileBuffers доталкивает данные до клиента
    // до отключения — гонка исчезает (проверено live, research §35).
    unsafe {
        FlushFileBuffers(handle);
        DisconnectNamedPipe(handle);
    }
    Ok(())
}

/// SID процесса-клиента pipe; `None`, если процесс недоступен.
fn client_process_sid(handle: HANDLE) -> Option<String> {
    let mut pid = 0u32;
    if unsafe { GetNamedPipeClientProcessId(handle, &mut pid) } == 0 {
        return None;
    }
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let sid = process_token_sid(process);
        CloseHandle(process);
        sid
    }
}

unsafe fn process_token_sid(process: HANDLE) -> Option<String> {
    let mut token: HANDLE = std::ptr::null_mut();
    if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
        return None;
    }
    let mut size = 0u32;
    GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
    // Буфер из u64: гарантирует выравнивание под TOKEN_USER, в отличие от Vec<u8>.
    let words = (size as usize).div_ceil(std::mem::size_of::<u64>());
    let mut buffer = vec![0u64; words.max(1)];
    let ok = GetTokenInformation(
        token,
        TokenUser,
        buffer.as_mut_ptr().cast(),
        size,
        &mut size,
    );
    CloseHandle(token);
    if ok == 0 {
        return None;
    }
    let user = &*(buffer.as_ptr() as *const TOKEN_USER);
    sid_to_string(user.User.Sid)
}

unsafe fn sid_to_string(sid: *mut c_void) -> Option<String> {
    let mut raw: *mut u16 = std::ptr::null_mut();
    if ConvertSidToStringSidW(sid, &mut raw) == 0 {
        return None;
    }
    let mut length = 0usize;
    while *raw.add(length) != 0 {
        length += 1;
    }
    let value = String::from_utf16_lossy(std::slice::from_raw_parts(raw, length));
    LocalFree(raw as *mut c_void);
    Some(value)
}

/// SDDL для pipe: только владелец, SYSTEM и Administrators.
pub fn pipe_sddl(allowed_sid: &str) -> String {
    format!("D:(A;;GA;;;{allowed_sid})(A;;GA;;;SY)(A;;GA;;;BA)")
}

/// Полный путь named pipe.
pub fn pipe_path(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

struct SecurityDescriptor {
    raw: PSECURITY_DESCRIPTOR,
}

impl SecurityDescriptor {
    fn from_sddl(sddl: &str) -> anyhow::Result<Self> {
        let wide = to_wide(sddl);
        let mut raw: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let mut size = 0u32;
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut raw,
                &mut size,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error()).context("SDDL не разобран");
        }
        Ok(Self { raw })
    }

    fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
        self.raw
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                LocalFree(self.raw);
            }
        }
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sddl_grants_only_user_system_and_admins() {
        let sddl = pipe_sddl("S-1-5-21-1-2-3-1001");
        assert_eq!(
            sddl,
            "D:(A;;GA;;;S-1-5-21-1-2-3-1001)(A;;GA;;;SY)(A;;GA;;;BA)"
        );
        assert!(!sddl.contains(";;;WD)"), "Everyone не должен иметь доступ");
        assert!(!sddl.contains(";;;AN)"), "анонимный доступ запрещён");
    }

    #[test]
    fn pipe_path_has_windows_prefix() {
        assert_eq!(
            pipe_path("splitroute-helper"),
            r"\\.\pipe\splitroute-helper"
        );
    }
}
