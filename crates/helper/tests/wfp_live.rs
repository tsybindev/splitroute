//! Live-проверка WFP kill switch на Windows — регресс на баг перечисления.
//!
//! Требует прав администратора: WFP-объекты создаются и удаляются по-настоящему.
//! Помечен `#[ignore]`, чтобы обычные прогоны тестов не зависели от elevation.
//! Запуск вручную из elevated-консоли:
//!   cargo test -p splitroute-helper --test wfp_live -- --ignored --nocapture
//!
//! Регресс (research §35): шаблон перечисления с GUID_NULL в layerKey
//! возвращает FWP_E_LAYER_NOT_FOUND (0x80320004) даже при наличии совпадающих
//! фильтров — перечислять нужно по конкретным слоям ALE_AUTH_CONNECT_V4/V6.

#![cfg(windows)]

use std::ptr;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::{
    FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterAdd0, FwpmFilterCreateEnumHandle0,
    FwpmFilterDeleteById0, FwpmFilterDestroyEnumHandle0, FwpmFilterEnum0, FwpmFreeMemory0,
    FwpmProviderAdd0, FwpmProviderDeleteByKey0, FwpmSubLayerAdd0, FwpmSubLayerDeleteByKey0,
    FWPM_ACTION0, FWPM_ACTION0_0, FWPM_DISPLAY_DATA0, FWPM_FILTER0, FWPM_FILTER0_0,
    FWPM_FILTER_ENUM_TEMPLATE0, FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_PROVIDER0, FWPM_SESSION0,
    FWPM_SUBLAYER0, FWP_ACTION_PERMIT, FWP_BYTE_BLOB, FWP_FILTER_ENUM_FULLY_CONTAINED, FWP_UINT64,
    FWP_VALUE0, FWP_VALUE0_0,
};
use windows_sys::Win32::System::Rpc::RPC_C_AUTHN_WINNT;

fn open_engine() -> HANDLE {
    let session = FWPM_SESSION0 {
        sessionKey: GUID::from_u128(0),
        displayData: FWPM_DISPLAY_DATA0 {
            name: ptr::null_mut(),
            description: ptr::null_mut(),
        },
        flags: 0,
        txnWaitTimeoutInMSec: 15_000,
        processId: 0,
        sid: ptr::null_mut(),
        username: ptr::null_mut(),
        kernelMode: 0,
    };
    let mut engine: HANDLE = ptr::null_mut();
    let r = unsafe {
        FwpmEngineOpen0(
            ptr::null(),
            RPC_C_AUTHN_WINNT,
            ptr::null(),
            &session,
            &mut engine,
        )
    };
    assert_eq!(r, NO_ERROR, "FwpmEngineOpen0: 0x{r:08X} (нужен elevation)");
    engine
}

/// Перечисление фильтров провайдера в конкретном слое — как в фиксе
/// `delete_objects`. Пустой результат на валидном слое — ОК с нулём.
fn enum_layer(engine: HANDLE, layer: GUID) -> Result<Vec<u64>, u32> {
    let mut template: FWPM_FILTER_ENUM_TEMPLATE0 = unsafe { std::mem::zeroed() };
    template.providerKey = &PROVIDER_KEY as *const GUID as *mut GUID;
    template.layerKey = layer;
    template.enumType = FWP_FILTER_ENUM_FULLY_CONTAINED;
    template.actionMask = u32::MAX;

    let mut handle: HANDLE = ptr::null_mut();
    let r = unsafe { FwpmFilterCreateEnumHandle0(engine, &template, &mut handle) };
    if r != 0 {
        return Err(r);
    }
    let mut ids = Vec::new();
    loop {
        let mut entries: *mut *mut FWPM_FILTER0 = ptr::null_mut();
        let mut count: u32 = 0;
        let r = unsafe { FwpmFilterEnum0(engine, handle, 64, &mut entries, &mut count) };
        assert_eq!(r, 0, "FwpmFilterEnum0: 0x{r:08X}");
        if count == 0 || entries.is_null() {
            if !entries.is_null() {
                let mut raw = entries as *mut std::ffi::c_void;
                unsafe { FwpmFreeMemory0(&mut raw) };
            }
            break;
        }
        for index in 0..count as usize {
            let filter = unsafe { *entries.add(index) };
            if !filter.is_null() {
                ids.push(unsafe { (*filter).filterId });
            }
        }
        let mut raw = entries as *mut std::ffi::c_void;
        unsafe { FwpmFreeMemory0(&mut raw) };
        if count < 64 {
            break;
        }
    }
    unsafe { FwpmFilterDestroyEnumHandle0(engine, handle) };
    Ok(ids)
}

fn add_permit_filter(engine: HANDLE) -> u64 {
    let name = "splitroute-wfp-live-test\0"
        .encode_utf16()
        .collect::<Vec<u16>>();
    let display = FWPM_DISPLAY_DATA0 {
        name: name.as_ptr() as *mut u16,
        description: ptr::null_mut(),
    };
    let mut weight_value = 1u64;
    let blob = FWP_BYTE_BLOB {
        size: 0,
        data: ptr::null_mut(),
    };
    let filter = FWPM_FILTER0 {
        filterKey: GUID::from_u128(0),
        displayData: display,
        flags: 0,
        providerKey: &PROVIDER_KEY as *const GUID as *mut GUID,
        providerData: blob,
        layerKey: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        subLayerKey: SUBLAYER_KEY,
        weight: FWP_VALUE0 {
            r#type: FWP_UINT64,
            Anonymous: FWP_VALUE0_0 {
                uint64: &mut weight_value,
            },
        },
        numFilterConditions: 0,
        filterCondition: ptr::null_mut(),
        action: FWPM_ACTION0 {
            r#type: FWP_ACTION_PERMIT,
            Anonymous: FWPM_ACTION0_0 {
                filterType: GUID::from_u128(0),
            },
        },
        Anonymous: FWPM_FILTER0_0 { rawContext: 0 },
        reserved: ptr::null_mut(),
        filterId: 0,
        effectiveWeight: FWP_VALUE0 {
            r#type: 0,
            Anonymous: FWP_VALUE0_0 {
                uint64: ptr::null_mut(),
            },
        },
    };
    let mut id = 0u64;
    let r = unsafe { FwpmFilterAdd0(engine, &filter, ptr::null_mut(), &mut id) };
    assert_eq!(r, 0, "FwpmFilterAdd0: 0x{r:08X}");
    id
}

// PROVIDER_KEY/SUBLAYER_KEY — ключи helper'а. Единственный источник истины —
// константы в crates/helper/src/killswitch/windows.rs; здесь они повторяются,
// потому что приватные константы крейта из интеграционного теста не видны.
const PROVIDER_KEY: GUID = GUID::from_u128(0x6a4c2f9e_1b7d_4c3e_9f2a_5d8b7e6c1a20);
const SUBLAYER_KEY: GUID = GUID::from_u128(0x6a4c2f9e_1b7d_4c3e_9f2a_5d8b7e6c1a21);

#[test]
#[ignore = "нужен elevation и реальные WFP-объекты: cargo test -p splitroute-helper --test wfp_live -- --ignored"]
fn per_layer_enumeration_finds_and_removes_our_filters() {
    let engine = open_engine();

    // Провайдер и подслой с теми же ключами, что у helper'а.
    let name = "SplitRoute kill switch\0"
        .encode_utf16()
        .collect::<Vec<u16>>();
    let display = FWPM_DISPLAY_DATA0 {
        name: name.as_ptr() as *mut u16,
        description: ptr::null_mut(),
    };
    let provider = FWPM_PROVIDER0 {
        providerKey: PROVIDER_KEY,
        displayData: display,
        flags: 0,
        providerData: FWP_BYTE_BLOB {
            size: 0,
            data: ptr::null_mut(),
        },
        serviceName: ptr::null_mut(),
    };
    let r = unsafe { FwpmProviderAdd0(engine, &provider, ptr::null_mut()) };
    assert!(r == 0 || r == 0x80320009, "FwpmProviderAdd0: 0x{r:08X}");
    let sublayer = FWPM_SUBLAYER0 {
        subLayerKey: SUBLAYER_KEY,
        displayData: display,
        flags: 0,
        providerKey: &PROVIDER_KEY as *const GUID as *mut GUID,
        providerData: FWP_BYTE_BLOB {
            size: 0,
            data: ptr::null_mut(),
        },
        weight: u16::MAX,
    };
    let r = unsafe { FwpmSubLayerAdd0(engine, &sublayer, ptr::null_mut()) };
    assert!(r == 0 || r == 0x80320009, "FwpmSubLayerAdd0: 0x{r:08X}");

    // Фильтр-приманка в слое V4: старый шаблон (layerKey GUID_NULL) его не
    // находил — баг 0x80320004; новый по слоям обязан найти.
    let id = add_permit_filter(engine);

    let ids = enum_layer(engine, FWPM_LAYER_ALE_AUTH_CONNECT_V4).expect("перечисление V4 слоя");
    assert!(
        ids.contains(&id),
        "фильтр {id} не найден перечислением по слою V4"
    );

    // Удаляем всё в порядке helper'а: фильтры -> подслой -> провайдер.
    for found in ids {
        let r = unsafe { FwpmFilterDeleteById0(engine, found) };
        assert_eq!(r, 0, "FwpmFilterDeleteById0({found}): 0x{r:08X}");
    }
    let r = unsafe { FwpmSubLayerDeleteByKey0(engine, &SUBLAYER_KEY) };
    assert_eq!(r, 0, "подслой не удалился (держит фильтр?): 0x{r:08X}");
    let r = unsafe { FwpmProviderDeleteByKey0(engine, &PROVIDER_KEY) };
    assert_eq!(r, 0, "провайдер не удалился: 0x{r:08X}");

    // Пустой результат на валидном слое — ОК с нулем, не 0x80320004.
    let after =
        enum_layer(engine, FWPM_LAYER_ALE_AUTH_CONNECT_V4).expect("перечисление после чистки");
    assert!(after.is_empty(), "после чистки остались фильтры: {after:?}");

    unsafe { FwpmEngineClose0(engine) };
}
