//! Проверки конфигурации упаковки.
//!
//! `externalBin` заставляет `tauri-build` искать sidecar ещё на этапе
//! `cargo check`, а бинари в git не попадают (`.gitignore`). Поэтому
//! `externalBin` должен жить только в платформенных конфигах: иначе чистый
//! клон не соберётся. Эти тесты фиксируют инвариант.

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("не удалось прочитать {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("невалидный JSON в {}: {error}", path.display()))
}

#[test]
fn base_config_has_no_external_bin() {
    let config = read_json(&manifest_dir().join("tauri.conf.json"));
    assert!(
        config["bundle"].get("externalBin").is_none(),
        "externalBin в базовом конфиге сломает `cargo check` на чистом клоне"
    );
}

#[test]
fn base_config_keeps_bundling_off() {
    let config = read_json(&manifest_dir().join("tauri.conf.json"));
    assert_eq!(
        config["bundle"]["active"], false,
        "базовая сборка не должна бандлить приложение без sidecar-бинарей"
    );
}

#[test]
fn linux_config_bundles_helper_and_core() {
    let config = read_json(&manifest_dir().join("tauri.linux.conf.json"));
    let binaries = config["bundle"]["externalBin"]
        .as_array()
        .expect("externalBin в linux-конфиге");
    let listed: Vec<&str> = binaries.iter().filter_map(|value| value.as_str()).collect();
    assert!(listed.contains(&"binaries/sing-box"));
    assert!(listed.contains(&"binaries/splitroute-helper"));

    assert_eq!(config["bundle"]["active"], true);
}

#[test]
fn windows_config_bundles_helper_and_core() {
    let config = read_json(&manifest_dir().join("tauri.windows.conf.json"));
    let binaries = config["bundle"]["externalBin"]
        .as_array()
        .expect("externalBin в windows-конфиге");
    let listed: Vec<&str> = binaries.iter().filter_map(|value| value.as_str()).collect();
    assert!(listed.contains(&"binaries/sing-box"));
    assert!(listed.contains(&"binaries/splitroute-helper"));

    assert_eq!(config["bundle"]["active"], true);
}

#[test]
fn platform_configs_name_existing_targets() {
    let linux = read_json(&manifest_dir().join("tauri.linux.conf.json"));
    let targets: Vec<&str> = linux["bundle"]["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    for expected in ["deb", "rpm", "appimage"] {
        assert!(targets.contains(&expected), "нет target {expected}");
    }

    let windows = read_json(&manifest_dir().join("tauri.windows.conf.json"));
    let targets: Vec<&str> = windows["bundle"]["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(targets.contains(&"nsis"));
}

#[test]
fn packaging_scripts_are_valid_shell() {
    for script in [
        "../scripts/package.sh",
        "../scripts/build-helper-sidecar.sh",
    ] {
        let path = manifest_dir().join(script);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("нет {}: {error}", path.display()));
        assert!(
            text.starts_with("#!/usr/bin/env sh"),
            "{script}: ожидался POSIX sh"
        );
        let status = std::process::Command::new("/bin/sh")
            .arg("-n")
            .arg(&path)
            .status()
            .expect("запуск sh -n");
        assert!(status.success(), "{script} не проходит sh -n");
    }
}
