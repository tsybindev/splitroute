//! Служба Windows для helper'а.
//!
//! Служба запускается как `splitroute-helper-service` под SYSTEM и создаёт
//! named pipe с ACL на SID пользователя. Установку/удаление службы выполняет
//! установщик приложения через UAC (`src-tauri/src/privileged/windows.rs`);
//! сам helper не умеет создавать службы — только обслуживать вызовы
//! диспетчера служб и клиентов pipe.
//!
//! Аргументы службы передаются в образе так:
//! `splitroute-helper.exe service --pipe <имя> --runtime-dir <каталог>
//! --sing-box <бинарь> --allow-sid <sid>`.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::{
    Foundation::{ERROR_FAILED_SERVICE_CONTROLLER_CONNECT, NO_ERROR},
    System::Services::{
        RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
        SERVICE_ACCEPT_STOP, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING,
        SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_STOPPED, SERVICE_STOP_PENDING,
        SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
    },
};

use crate::args::{parse_args, split_subcommand, RawOptions, Subcommand};
use crate::pipe::{self, ServerOptions};

/// Имя службы, совпадает с тем, что создаёт установщик.
pub const SERVICE_NAME: &str = "splitroute-helper-service";

/// Параметры службы, подготовленные до входа в диспетчер.
static OPTIONS: OnceLock<Result<ServerOptions, String>> = OnceLock::new();

/// Handle статуса службы. `SERVICE_STATUS_HANDLE` — сырой указатель, поэтому
/// оборачиваем его в тип с `Send + Sync`: диспетчер и обработчик контроля
/// вызываются на разных потоках службы.
struct StatusHandle(SERVICE_STATUS_HANDLE);

unsafe impl Send for StatusHandle {}
unsafe impl Sync for StatusHandle {}

static STATUS_HANDLE: OnceLock<StatusHandle> = OnceLock::new();

/// Точка входа helper'а на Windows.
///
/// Подкоманды:
/// - `service` — работа под диспетчером служб (устанавливается установщиком);
/// - `console` — запуск в консоли от администратора, без регистрации службы
///   (dev-режим и portable-сборка).
pub fn dispatch(args: Vec<String>) -> Result<(), String> {
    let (subcommand, rest) = split_subcommand(args)?;
    let raw = parse_args(rest)?;

    match subcommand {
        Subcommand::Service => {
            let options = build_options(&raw);
            let _ = OPTIONS.set(options.clone());
            options?;
            run_service_dispatcher()
        }
        Subcommand::Console => {
            let options = build_options(&raw)?;
            eprintln!("splitroute-helper: консольный режим, служба не регистрируется");
            pipe::run(options).map_err(|error| format!("{error:#}"))
        }
    }
}

fn build_options(raw: &RawOptions) -> Result<ServerOptions, String> {
    let pipe_name = raw.pipe.clone().ok_or("не указан --pipe")?;
    let runtime_dir = raw.runtime_dir.clone().ok_or("не указан --runtime-dir")?;
    let sing_box = raw.sing_box.clone().ok_or("не указан --sing-box")?;
    let allowed_sid = raw.allow_sid.clone().ok_or("не указан --allow-sid")?;
    if !sing_box.is_file() {
        return Err(format!("sing-box не найден: {}", sing_box.display()));
    }
    Ok(ServerOptions {
        pipe_name,
        runtime_dir,
        sing_box,
        allowed_sid,
    })
}

fn run_service_dispatcher() -> Result<(), String> {
    let service_name = to_wide(SERVICE_NAME);
    let mut table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: service_name.as_ptr() as *mut u16,
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: std::ptr::null_mut(),
            lpServiceProc: None,
        },
    ];

    let ok = unsafe { StartServiceCtrlDispatcherW(table.as_mut_ptr()) };
    if ok == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT as i32) {
            return Err(
                "helper запущен не диспетчером служб: служба ставится установщиком, \
                 а не запускается вручную"
                    .to_string(),
            );
        }
        return Err(format!("StartServiceCtrlDispatcherW: {error}"));
    }
    Ok(())
}

extern "system" fn service_main(_argc: u32, _argv: *mut *mut u16) {
    let name = to_wide(SERVICE_NAME);
    let handle = unsafe {
        RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(handle_control), std::ptr::null_mut())
    };
    if handle.is_null() {
        return;
    }
    let _ = STATUS_HANDLE.set(StatusHandle(handle));
    report_status(SERVICE_START_PENDING, NO_ERROR);

    let options = match OPTIONS.get() {
        Some(Ok(options)) => options.clone(),
        _ => {
            report_status(SERVICE_STOPPED, NO_ERROR);
            return;
        }
    };

    report_status(SERVICE_RUNNING, NO_ERROR);
    if let Err(error) = pipe::run(options) {
        eprintln!("splitroute-helper: служба остановлена: {error:#}");
    }
    report_status(SERVICE_STOPPED, NO_ERROR);
}

extern "system" fn handle_control(
    control: u32,
    _event_type: u32,
    _event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    if control == SERVICE_CONTROL_STOP {
        report_status(SERVICE_STOP_PENDING, NO_ERROR);
        pipe::request_shutdown();
    }
    NO_ERROR
}

fn report_status(state: u32, exit_code: u32) {
    let Some(handle) = STATUS_HANDLE.get() else {
        return;
    };
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: SERVICE_ACCEPT_STOP,
        dwWin32ExitCode: exit_code,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    unsafe {
        SetServiceStatus(handle.0, &status);
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Аргументы командной строки службы для установщика (без секретов).
pub fn service_arguments(options: &ServerOptions) -> Vec<String> {
    vec![
        "service".to_string(),
        "--pipe".to_string(),
        options.pipe_name.clone(),
        "--runtime-dir".to_string(),
        options.runtime_dir.display().to_string(),
        "--sing-box".to_string(),
        options.sing_box.display().to_string(),
        "--allow-sid".to_string(),
        options.allowed_sid.clone(),
    ]
}
