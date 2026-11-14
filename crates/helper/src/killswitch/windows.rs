//! Windows: правила kill switch в Windows Filtering Platform (WFP).
//!
//! Используется собственный провайдер и подслой WFP: фильтры не пересекаются с
//! правилами брандмауэра Windows и антивирусов. Сессия не динамическая, поэтому
//! правила переживают падение helper'а; при следующем старте helper удаляет их
//! (`cleanup_stale`). Ставить правила через `netsh advfirewall` нельзя: в
//! брандмауэре Windows блокирующие правила приоритетнее разрешающих, поэтому
//! «разрешить ядро и TUN, заблокировать остальное» без смены глобальной политики
//! не выражается, а смена политики ломает систему.
//!
//! Условия фильтров описаны в [`super::rules::WINDOWS_FILTERS`] и не содержат
//! данных от клиента. Единственное значение, зависящее от установки, — AppId
//! процесса ядра (`sing-box.exe`): путь helper получает из `--sing-box`.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    FWP_E_ALREADY_EXISTS, FWP_E_FILTER_NOT_FOUND, FWP_E_NOT_FOUND, FWP_E_PROVIDER_NOT_FOUND,
    FWP_E_SUBLAYER_NOT_FOUND, HANDLE,
};
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::{
    FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterAdd0, FwpmFilterCreateEnumHandle0,
    FwpmFilterDeleteById0, FwpmFilterDestroyEnumHandle0, FwpmFilterEnum0, FwpmFreeMemory0,
    FwpmGetAppIdFromFileName0, FwpmProviderAdd0, FwpmProviderDeleteByKey0, FwpmSubLayerAdd0,
    FwpmSubLayerDeleteByKey0, FwpmTransactionAbort0, FwpmTransactionBegin0, FwpmTransactionCommit0,
    FWPM_ACTION0, FWPM_ACTION0_0, FWPM_CONDITION_ALE_APP_ID, FWPM_CONDITION_FLAGS,
    FWPM_CONDITION_IP_LOCAL_ADDRESS, FWPM_CONDITION_IP_LOCAL_PORT, FWPM_CONDITION_IP_PROTOCOL,
    FWPM_CONDITION_IP_REMOTE_ADDRESS, FWPM_CONDITION_IP_REMOTE_PORT, FWPM_DISPLAY_DATA0,
    FWPM_FILTER0, FWPM_FILTER0_0, FWPM_FILTER_CONDITION0, FWPM_FILTER_ENUM_TEMPLATE0,
    FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6, FWPM_PROVIDER0, FWPM_SESSION0,
    FWPM_SUBLAYER0, FWP_ACTION_BLOCK, FWP_ACTION_PERMIT, FWP_BYTE_BLOB, FWP_BYTE_BLOB_TYPE,
    FWP_CONDITION_FLAG_IS_LOOPBACK, FWP_CONDITION_VALUE0, FWP_CONDITION_VALUE0_0, FWP_EMPTY,
    FWP_FILTER_ENUM_FULLY_CONTAINED, FWP_MATCH_EQUAL, FWP_MATCH_FLAGS_ALL_SET, FWP_UINT16,
    FWP_UINT32, FWP_UINT64, FWP_UINT8, FWP_V4_ADDR_AND_MASK, FWP_V4_ADDR_MASK,
    FWP_V6_ADDR_AND_MASK, FWP_V6_ADDR_MASK, FWP_VALUE0, FWP_VALUE0_0,
};
use windows_sys::Win32::System::Rpc::RPC_C_AUTHN_WINNT;

use super::rules::{WinAction, WinCondition, WinFilter, WinLayer, WINDOWS_FILTERS};
use super::{Firewall, FirewallError};

/// Собственный провайдер WFP: по нему helper удаляет все свои объекты.
const PROVIDER_KEY: GUID = GUID::from_u128(0x6a4c2f9e_1b7d_4c3e_9f2a_5d8b7e6c1a20);
/// Собственный подслой: вес максимальный, чтобы блок не перекрывался правилами
/// брандмауэра Windows, а разрешения внутри подслоя были выше блока.
const SUBLAYER_KEY: GUID = GUID::from_u128(0x6a4c2f9e_1b7d_4c3e_9f2a_5d8b7e6c1a21);

/// Сколько фильтров запрашивать за один вызов перечисления.
const ENUM_PAGE: u32 = 64;

/// Платформенный backend WFP.
pub struct SystemFirewall {
    /// WFP не пишет файлов; каталог хранится для единообразия с pf/nft.
    _runtime_dir: PathBuf,
    /// Путь к ядру: из него строится AppId, единственное «динамическое» условие.
    core: Option<PathBuf>,
}

impl SystemFirewall {
    pub fn new(runtime_dir: &Path, core: Option<&Path>) -> Self {
        Self {
            _runtime_dir: runtime_dir.to_path_buf(),
            core: core.map(Path::to_path_buf),
        }
    }

    fn core_path(&self) -> Result<&Path, FirewallError> {
        self.core.as_deref().ok_or_else(|| {
            "не удалось определить путь к ядру для правил WFP: нужен --sing-box".to_string()
        })
    }
}

impl Firewall for SystemFirewall {
    fn enable(&self) -> Result<(), FirewallError> {
        let core = self.core_path()?;
        let engine = open_engine()?;
        if let Err(error) = begin(engine) {
            close(engine);
            return Err(error);
        }

        let result = delete_objects(engine).and_then(|()| add_objects(engine, core));
        let result = match result {
            Ok(()) => commit(engine),
            Err(error) => {
                abort(engine);
                Err(error)
            }
        };

        close(engine);
        result
    }

    fn disable(&self) -> Result<(), FirewallError> {
        let engine = open_engine()?;
        if let Err(error) = begin(engine) {
            close(engine);
            return Err(error);
        }

        let result = match delete_objects(engine) {
            Ok(()) => commit(engine),
            Err(error) => {
                abort(engine);
                Err(error)
            }
        };

        close(engine);
        result
    }
}

fn open_engine() -> Result<HANDLE, FirewallError> {
    let session = FWPM_SESSION0 {
        sessionKey: GUID::from_u128(0),
        displayData: FWPM_DISPLAY_DATA0 {
            name: ptr::null_mut(),
            description: ptr::null_mut(),
        },
        // Не dynamic: правила должны оставаться при падении helper'а.
        flags: 0,
        txnWaitTimeoutInMSec: 15_000,
        processId: 0,
        sid: ptr::null_mut(),
        username: ptr::null_mut(),
        kernelMode: 0,
    };
    let mut engine: HANDLE = ptr::null_mut();
    let result = unsafe {
        FwpmEngineOpen0(
            ptr::null(),
            RPC_C_AUTHN_WINNT,
            ptr::null(),
            &session,
            &mut engine,
        )
    };
    if result != 0 {
        return Err(wfp_error("не удалось открыть WFP", result));
    }
    Ok(engine)
}

fn begin(engine: HANDLE) -> Result<(), FirewallError> {
    let result = unsafe { FwpmTransactionBegin0(engine, 0) };
    if result != 0 {
        return Err(wfp_error("не удалось начать транзакцию WFP", result));
    }
    Ok(())
}

fn commit(engine: HANDLE) -> Result<(), FirewallError> {
    let result = unsafe { FwpmTransactionCommit0(engine) };
    if result != 0 {
        return Err(wfp_error("не удалось применить транзакцию WFP", result));
    }
    Ok(())
}

fn abort(engine: HANDLE) {
    unsafe {
        FwpmTransactionAbort0(engine);
    }
}

fn close(engine: HANDLE) {
    unsafe {
        FwpmEngineClose0(engine);
    }
}

/// Удаляет все объекты helper'а: фильтры, подслой, провайдер.
///
/// Отсутствие объектов — не ошибка: снятие идемпотентно. Порядок обязателен:
/// объект нельзя удалить, пока на него ссылаются (WFP Object Management).
fn delete_objects(engine: HANDLE) -> Result<(), FirewallError> {
    let mut filter_ids = Vec::new();
    let mut template: FWPM_FILTER_ENUM_TEMPLATE0 = unsafe { std::mem::zeroed() };
    template.providerKey = &PROVIDER_KEY as *const GUID as *mut GUID;
    template.enumType = FWP_FILTER_ENUM_FULLY_CONTAINED;
    template.actionMask = u32::MAX;

    let mut enum_handle: HANDLE = ptr::null_mut();
    let result = unsafe { FwpmFilterCreateEnumHandle0(engine, &template, &mut enum_handle) };
    if result != 0 && !is_code(result, FWP_E_NOT_FOUND) {
        return Err(wfp_error("не удалось перечислить правила WFP", result));
    }

    if result == 0 {
        let enumerated = (|| -> Result<(), FirewallError> {
            loop {
                let mut entries: *mut *mut FWPM_FILTER0 = ptr::null_mut();
                let mut count: u32 = 0;
                let result = unsafe {
                    FwpmFilterEnum0(engine, enum_handle, ENUM_PAGE, &mut entries, &mut count)
                };
                if result != 0 {
                    return Err(wfp_error("не удалось прочитать правила WFP", result));
                }
                if count == 0 || entries.is_null() {
                    if !entries.is_null() {
                        let mut raw = entries as *mut c_void;
                        unsafe { FwpmFreeMemory0(&mut raw) };
                    }
                    return Ok(());
                }
                for index in 0..count as usize {
                    let filter = unsafe { *entries.add(index) };
                    if !filter.is_null() {
                        filter_ids.push(unsafe { (*filter).filterId });
                    }
                }
                let mut raw = entries as *mut c_void;
                unsafe { FwpmFreeMemory0(&mut raw) };
                if count < ENUM_PAGE {
                    return Ok(());
                }
            }
        })();
        unsafe { FwpmFilterDestroyEnumHandle0(engine, enum_handle) };
        enumerated?;
    }

    for id in filter_ids {
        let result = unsafe { FwpmFilterDeleteById0(engine, id) };
        if result != 0 && !is_code(result, FWP_E_FILTER_NOT_FOUND) {
            return Err(wfp_error("не удалось удалить правило WFP", result));
        }
    }

    let result = unsafe { FwpmSubLayerDeleteByKey0(engine, &SUBLAYER_KEY) };
    if result != 0 && !is_code(result, FWP_E_SUBLAYER_NOT_FOUND) {
        return Err(wfp_error("не удалось удалить подслой WFP", result));
    }

    let result = unsafe { FwpmProviderDeleteByKey0(engine, &PROVIDER_KEY) };
    if result != 0 && !is_code(result, FWP_E_PROVIDER_NOT_FOUND) {
        return Err(wfp_error("не удалось удалить провайдер WFP", result));
    }

    Ok(())
}

fn add_objects(engine: HANDLE, core: &Path) -> Result<(), FirewallError> {
    let provider_name = wide("SplitRoute kill switch");
    let provider_description = wide("SplitRoute kill switch provider");
    let display = FWPM_DISPLAY_DATA0 {
        name: provider_name.as_ptr() as *mut u16,
        description: provider_description.as_ptr() as *mut u16,
    };

    let provider = FWPM_PROVIDER0 {
        providerKey: PROVIDER_KEY,
        displayData: display,
        flags: 0,
        providerData: empty_blob(),
        serviceName: ptr::null_mut(),
    };
    let result = unsafe { FwpmProviderAdd0(engine, &provider, ptr::null_mut()) };
    if result != 0 && !is_code(result, FWP_E_ALREADY_EXISTS) {
        return Err(wfp_error("не удалось создать провайдер WFP", result));
    }

    let sublayer = FWPM_SUBLAYER0 {
        subLayerKey: SUBLAYER_KEY,
        displayData: display,
        flags: 0,
        providerKey: &PROVIDER_KEY as *const GUID as *mut GUID,
        providerData: empty_blob(),
        weight: u16::MAX,
    };
    let result = unsafe { FwpmSubLayerAdd0(engine, &sublayer, ptr::null_mut()) };
    if result != 0 && !is_code(result, FWP_E_ALREADY_EXISTS) {
        return Err(wfp_error("не удалось создать подслой WFP", result));
    }

    let app_id = app_id_blob(core)?;
    let result = add_filters(engine, app_id);
    free_app_id(app_id);
    result
}

fn add_filters(engine: HANDLE, app_id: *mut FWP_BYTE_BLOB) -> Result<(), FirewallError> {
    for plan in WINDOWS_FILTERS {
        add_filter(engine, plan, app_id)?;
    }
    Ok(())
}

fn add_filter(
    engine: HANDLE,
    plan: &WinFilter,
    app_id: *mut FWP_BYTE_BLOB,
) -> Result<(), FirewallError> {
    // `_v4`/`_v6` держат значения условий в куче: адреса не меняются, пока
    // фильтр не добавлен.
    let (mut conditions, _v4, _v6) = build_conditions(plan, app_id);
    let mut display_name = wide(plan.name);
    let display = FWPM_DISPLAY_DATA0 {
        name: display_name.as_mut_ptr(),
        description: ptr::null_mut(),
    };

    let layer = match plan.layer {
        WinLayer::AuthConnectV4 => FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        WinLayer::AuthConnectV6 => FWPM_LAYER_ALE_AUTH_CONNECT_V6,
    };
    let action_type = match plan.action {
        WinAction::Permit => FWP_ACTION_PERMIT,
        WinAction::Block => FWP_ACTION_BLOCK,
    };
    let mut weight_value = u64::from(plan.weight);
    let weight = FWP_VALUE0 {
        r#type: FWP_UINT64,
        Anonymous: FWP_VALUE0_0 {
            uint64: &mut weight_value,
        },
    };
    let action = FWPM_ACTION0 {
        r#type: action_type,
        Anonymous: FWPM_ACTION0_0 {
            filterType: GUID::from_u128(0),
        },
    };

    let filter = FWPM_FILTER0 {
        filterKey: GUID::from_u128(0),
        displayData: display,
        flags: 0,
        providerKey: &PROVIDER_KEY as *const GUID as *mut GUID,
        providerData: empty_blob(),
        layerKey: layer,
        subLayerKey: SUBLAYER_KEY,
        weight,
        numFilterConditions: conditions.len() as u32,
        filterCondition: conditions.as_mut_ptr(),
        action,
        Anonymous: FWPM_FILTER0_0 { rawContext: 0 },
        reserved: ptr::null_mut(),
        filterId: 0,
        effectiveWeight: empty_value(),
    };

    let mut filter_id = 0u64;
    let result = unsafe { FwpmFilterAdd0(engine, &filter, ptr::null_mut(), &mut filter_id) };
    if result != 0 {
        return Err(wfp_error(
            &format!("не удалось добавить правило {}", plan.name),
            result,
        ));
    }
    Ok(())
}

// Адреса условий должны оставаться стабильными, пока WFP не скопировал фильтр:
// `Vec<Box<_>>` даёт стабильные адреса, `Vec<_>` мог бы переехать при росте.
#[allow(clippy::vec_box)]
fn build_conditions(
    plan: &WinFilter,
    app_id: *mut FWP_BYTE_BLOB,
) -> (
    Vec<FWPM_FILTER_CONDITION0>,
    Vec<Box<FWP_V4_ADDR_AND_MASK>>,
    Vec<Box<FWP_V6_ADDR_AND_MASK>>,
) {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    let mut conditions = Vec::with_capacity(plan.conditions.len());

    for condition in plan.conditions {
        let mut entry = empty_condition();
        match condition {
            WinCondition::Loopback => {
                entry.fieldKey = FWPM_CONDITION_FLAGS;
                entry.matchType = FWP_MATCH_FLAGS_ALL_SET;
                entry.conditionValue.r#type = FWP_UINT32;
                entry.conditionValue.Anonymous.uint32 = FWP_CONDITION_FLAG_IS_LOOPBACK;
            }
            WinCondition::TunLocalV4(addr) => {
                entry.fieldKey = FWPM_CONDITION_IP_LOCAL_ADDRESS;
                entry.conditionValue.r#type = FWP_UINT32;
                entry.conditionValue.Anonymous.uint32 = u32::from_be_bytes(*addr);
            }
            WinCondition::TunLocalV6(addr) => {
                v6.push(Box::new(FWP_V6_ADDR_AND_MASK {
                    addr: *addr,
                    prefixLength: 128,
                }));
                let value = v6.last_mut().expect("только что добавлен").as_mut();
                entry.fieldKey = FWPM_CONDITION_IP_LOCAL_ADDRESS;
                entry.conditionValue.r#type = FWP_V6_ADDR_MASK;
                entry.conditionValue.Anonymous.v6AddrMask = value as *mut FWP_V6_ADDR_AND_MASK;
            }
            WinCondition::RemoteV4 { addr, prefix } => {
                v4.push(Box::new(FWP_V4_ADDR_AND_MASK {
                    addr: u32::from_be_bytes(*addr),
                    mask: ipv4_mask(*prefix),
                }));
                let value = v4.last_mut().expect("только что добавлен").as_mut();
                entry.fieldKey = FWPM_CONDITION_IP_REMOTE_ADDRESS;
                entry.conditionValue.r#type = FWP_V4_ADDR_MASK;
                entry.conditionValue.Anonymous.v4AddrMask = value as *mut FWP_V4_ADDR_AND_MASK;
            }
            WinCondition::RemoteV6 { addr, prefix } => {
                v6.push(Box::new(FWP_V6_ADDR_AND_MASK {
                    addr: *addr,
                    prefixLength: *prefix,
                }));
                let value = v6.last_mut().expect("только что добавлен").as_mut();
                entry.fieldKey = FWPM_CONDITION_IP_REMOTE_ADDRESS;
                entry.conditionValue.r#type = FWP_V6_ADDR_MASK;
                entry.conditionValue.Anonymous.v6AddrMask = value as *mut FWP_V6_ADDR_AND_MASK;
            }
            WinCondition::RemoteExactV4(addr) => {
                entry.fieldKey = FWPM_CONDITION_IP_REMOTE_ADDRESS;
                entry.conditionValue.r#type = FWP_UINT32;
                entry.conditionValue.Anonymous.uint32 = u32::from_be_bytes(*addr);
            }
            WinCondition::Udp => {
                entry.fieldKey = FWPM_CONDITION_IP_PROTOCOL;
                entry.conditionValue.r#type = FWP_UINT8;
                entry.conditionValue.Anonymous.uint8 = 17;
            }
            WinCondition::LocalPort(port) => {
                entry.fieldKey = FWPM_CONDITION_IP_LOCAL_PORT;
                entry.conditionValue.r#type = FWP_UINT16;
                entry.conditionValue.Anonymous.uint16 = *port;
            }
            WinCondition::RemotePort(port) => {
                entry.fieldKey = FWPM_CONDITION_IP_REMOTE_PORT;
                entry.conditionValue.r#type = FWP_UINT16;
                entry.conditionValue.Anonymous.uint16 = *port;
            }
            WinCondition::CoreAppId => {
                entry.fieldKey = FWPM_CONDITION_ALE_APP_ID;
                entry.conditionValue.r#type = FWP_BYTE_BLOB_TYPE;
                entry.conditionValue.Anonymous.byteBlob = app_id;
            }
        }
        conditions.push(entry);
    }

    (conditions, v4, v6)
}

fn empty_condition() -> FWPM_FILTER_CONDITION0 {
    FWPM_FILTER_CONDITION0 {
        fieldKey: GUID::from_u128(0),
        matchType: FWP_MATCH_EQUAL,
        conditionValue: FWP_CONDITION_VALUE0 {
            r#type: FWP_EMPTY,
            Anonymous: FWP_CONDITION_VALUE0_0 {
                uint64: ptr::null_mut(),
            },
        },
    }
}

fn empty_value() -> FWP_VALUE0 {
    FWP_VALUE0 {
        r#type: FWP_EMPTY,
        Anonymous: FWP_VALUE0_0 {
            uint64: ptr::null_mut(),
        },
    }
}

fn empty_blob() -> FWP_BYTE_BLOB {
    FWP_BYTE_BLOB {
        size: 0,
        data: ptr::null_mut(),
    }
}

/// AppId процесса ядра. Значение выделяет WFP, освобождает `free_app_id`.
fn app_id_blob(core: &Path) -> Result<*mut FWP_BYTE_BLOB, FirewallError> {
    if !core.is_file() {
        return Err("не найден sing-box.exe: правило WFP не может разрешить ядро".to_string());
    }
    let path: Vec<u16> = core
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut blob: *mut FWP_BYTE_BLOB = ptr::null_mut();
    let result = unsafe { FwpmGetAppIdFromFileName0(path.as_ptr(), &mut blob) };
    if result != 0 || blob.is_null() {
        return Err(wfp_error("не удалось определить AppId ядра", result));
    }
    Ok(blob)
}

fn free_app_id(blob: *mut FWP_BYTE_BLOB) {
    if blob.is_null() {
        return;
    }
    let mut raw = blob as *mut c_void;
    unsafe { FwpmFreeMemory0(&mut raw) };
}

fn ipv4_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else if prefix >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix)
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn is_code(result: u32, code: i32) -> bool {
    result == code as u32
}

/// Безопасный текст ошибки: код WFP без сырых команд и данных.
fn wfp_error(action: &str, code: u32) -> FirewallError {
    format!("{action}: код WFP 0x{code:08X}")
}
