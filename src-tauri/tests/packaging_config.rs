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
fn windows_config_bundles_wintun_next_to_core() {
    // `wintun.dll` обязан оказаться рядом с `sing-box.exe`: иначе ядро не
    // поднимет TUN. На Windows `$RESOURCE` — это каталог исполняемого файла,
    // поэтому ресурс раскладывается в корень, а не в подкаталог.
    let config = read_json(&manifest_dir().join("tauri.windows.conf.json"));
    assert_eq!(
        config["bundle"]["resources"]["binaries/wintun.dll"], "wintun.dll",
        "wintun.dll должен попадать в каталог приложения"
    );
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
        "../scripts/portable.sh",
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

/// GPL-3.0 требует распространять текст лицензии вместе с программой, а
/// сторонние компоненты — с уведомлениями. Файлы должны лежать в корне и
/// попадать в поставку, а не только в репозиторий.
#[test]
fn license_and_notices_exist_in_repository_root() {
    let root = manifest_dir().join("..");
    for name in ["LICENSE", "THIRD_PARTY_NOTICES"] {
        let path = root.join(name);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("нет {}: {error}", path.display()));
        assert!(!text.trim().is_empty(), "{name} пуст");
    }

    let license = fs::read_to_string(root.join("LICENSE")).unwrap();
    assert!(
        license.contains("GNU GENERAL PUBLIC LICENSE") && license.contains("Version 3"),
        "LICENSE должен быть текстом GNU GPL v3"
    );

    let notices = fs::read_to_string(root.join("THIRD_PARTY_NOTICES")).unwrap();
    // sing-box — GPL-3.0-обязательство: версия, ссылка на исходники, лицензия.
    assert!(notices.contains("sing-box"), "в notices нет sing-box");
    assert!(notices.contains("v1.14.4"), "не закреплена версия sing-box");
    assert!(
        notices.contains("https://github.com/SagerNet/sing-box/tree/v1.14.4"),
        "нет ссылки на исходники закреплённой версии sing-box"
    );
    assert!(notices.contains("GPL-3.0"), "не указана лицензия sing-box");
    // Windows-драйвер распространяется по отдельной лицензии WireGuard LLC.
    assert!(notices.contains("Prebuilt Binaries License"));
    assert!(notices.contains("wintun"));
    // Frontend/Rust/Tauri зафиксированы.
    for component in ["Tauri", "react", "rustls"] {
        assert!(notices.contains(component), "в notices нет {component}");
    }
    // Уведомления не должны содержать секретов.
    assert!(!notices.contains("d811a5ee"));
}

#[test]
fn base_config_bundles_license_and_notices() {
    let config = read_json(&manifest_dir().join("tauri.conf.json"));
    assert_eq!(
        config["bundle"]["license"], "GPL-3.0-only",
        "в метаданных пакета должна быть SPDX-лицензия проекта"
    );
    assert_eq!(
        config["bundle"]["licenseFile"], "../LICENSE",
        "DMG/NSIS должны показывать текст лицензии"
    );
    assert_eq!(
        config["bundle"]["resources"]["../LICENSE"], "LICENSE",
        "LICENSE должен попадать в ресурсы приложения"
    );
    assert_eq!(
        config["bundle"]["resources"]["../THIRD_PARTY_NOTICES"], "THIRD_PARTY_NOTICES",
        "THIRD_PARTY_NOTICES должен попадать в ресурсы приложения"
    );
}

#[test]
fn linux_packages_ship_license_docs() {
    let config = read_json(&manifest_dir().join("tauri.linux.conf.json"));
    for (target, prefix) in [("deb", ""), ("rpm", "/"), ("appimage", "")] {
        let files = config["bundle"]["linux"][target]["files"]
            .as_object()
            .unwrap_or_else(|| panic!("нет bundle.linux.{target}.files"));
        for name in ["LICENSE", "THIRD_PARTY_NOTICES"] {
            let key = format!("{prefix}usr/share/doc/splitroute/{name}");
            assert!(
                files.contains_key(&key),
                "{target}: нет {key} — пакет не содержит уведомлений"
            );
            assert_eq!(files[&key], format!("../{name}"));
        }
    }
}
