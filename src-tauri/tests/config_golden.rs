//! Golden-тесты генератора конфига + проверка `sing-box check`.
//!
//! Golden-файлы обновляются командой:
//!     UPDATE_GOLDEN=1 cargo test -p splitroute --test config_golden
//! Без `UPDATE_GOLDEN` тест сравнивает с файлом и падает при расхождении.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use splitroute_lib::config::build_config;
use splitroute_protocol::{
    AppRule, DnsMode, LogLevel, Platform, Profile, Protocol, Settings, TlsConfig, TlsKind,
    Transport,
};

fn profile() -> Profile {
    Profile {
        id: "profile-1".to_string(),
        name: "Тестовая локация A".to_string(),
        protocol: Protocol::Vless,
        server: "203.0.113.10".to_string(),
        port: 443,
        uuid: "d811a5ee-1d55-4dca-bd7f-f68b768a2241".to_string(),
        flow: Some("xtls-rprx-vision".to_string()),
        tls: TlsConfig {
            kind: TlsKind::Reality,
            server_name: "example.com".to_string(),
            public_key: Some("V1OuvPqM8PJgzYhYvOESBeqkVqqAhIcqZOl778zbalU".to_string()),
            short_id: Some("00e5e640a81a9ac1".to_string()),
            fingerprint: Some("chrome".to_string()),
            alpn: Vec::new(),
        },
        transport: Transport::Tcp,
        raw: None,
    }
}

fn chrome_rule() -> AppRule {
    AppRule {
        id: "rule-chrome".to_string(),
        name: "Google Chrome".to_string(),
        platform: Platform::Macos,
        process_name: vec!["Google Chrome".to_string()],
        process_path: Vec::new(),
        path_regex: vec![r"^/Applications/Google Chrome\.app/".to_string()],
        enabled: true,
    }
}

fn curl_rule() -> AppRule {
    AppRule {
        id: "rule-curl".to_string(),
        name: "curl".to_string(),
        platform: Platform::Macos,
        process_name: vec!["curl".to_string()],
        process_path: Vec::new(),
        path_regex: Vec::new(),
        enabled: true,
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config")
}

fn check_golden(name: &str, config: &Value) {
    let path = golden_dir().join(format!("{name}.json"));
    let actual = serde_json::to_string_pretty(config).unwrap() + "\n";
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        fs::create_dir_all(golden_dir()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "golden {} не найден ({error}); запустите UPDATE_GOLDEN=1 cargo test",
            path.display()
        )
    });
    assert_eq!(actual, expected, "golden не совпал: {}", path.display());
}

#[test]
fn golden_default_allowlist_process_dns() {
    let config = build_config(
        &Settings::default(),
        &profile(),
        &[chrome_rule(), curl_rule()],
    )
    .unwrap();
    check_golden("default-allowlist", &config);
}

#[test]
fn golden_ipv6_on_system_dns() {
    let settings = Settings {
        ipv6_enabled: true,
        dns_mode: DnsMode::System,
        log_level: LogLevel::Debug,
        ..Settings::default()
    };
    let config = build_config(&settings, &profile(), &[chrome_rule()]).unwrap();
    check_golden("ipv6-on-system-dns", &config);
}

#[test]
fn golden_empty_allowlist() {
    let config = build_config(&Settings::default(), &profile(), &[]).unwrap();
    check_golden("empty-allowlist", &config);
}

#[test]
fn generated_configs_pass_sing_box_check() {
    let Some(binary) = find_sing_box() else {
        eprintln!("пропуск: sing-box не найден в src-tauri/binaries (scripts/fetch-core.sh)");
        return;
    };

    for (name, config) in [
        (
            "default",
            build_config(
                &Settings::default(),
                &profile(),
                &[chrome_rule(), curl_rule()],
            )
            .unwrap(),
        ),
        (
            "ipv6",
            build_config(
                &Settings {
                    ipv6_enabled: true,
                    ..Settings::default()
                },
                &profile(),
                &[chrome_rule()],
            )
            .unwrap(),
        ),
    ] {
        let path = std::env::temp_dir().join(format!(
            "splitroute-golden-{}-{}.json",
            std::process::id(),
            name
        ));
        fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
        let output = Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("не удалось запустить sing-box");
        let _ = fs::remove_file(&path);
        assert!(
            output.status.success(),
            "sing-box check провалился для {name}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn find_sing_box() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    entries.sort();
    entries.into_iter().next()
}
