//! Совместимость реального клиента приложения с бинарём helper'а.
//!
//! Тест поднимает `splitroute-helper` с фиктивным ядром и проходит путь
//! `UnixHelperClient::status/start/stop` по настоящему Unix-сокету. Это
//! проверяет, что формат протокола не разошёлся между крейтами.

#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use splitroute_lib::privileged::{HelperClient, UnixHelperClient};
use splitroute_protocol::CoreState;

const OK_CORE: &str = r#"#!/bin/sh
case "$1" in
  version) echo "sing-box version 1.14.4-test" ;;
  check) exit 0 ;;
  run)
    trap 'exit 0' TERM
    while :; do sleep 0.1; done
    ;;
  *) exit 1 ;;
esac
"#;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from("/tmp").join(format!(
            "sr-client-{tag}-{}-{sequence}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Helper {
    child: Option<Child>,
}

impl Helper {
    fn spawn(helper: &Path, socket: &Path, runtime: &Path, sing_box: &Path) -> Self {
        let child = Command::new(helper)
            .arg("--socket")
            .arg(socket)
            .arg("--runtime-dir")
            .arg(runtime)
            .arg("--sing-box")
            .arg(sing_box)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("запуск splitroute-helper");
        Self { child: Some(child) }
    }

    fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
            let _ = child.wait();
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[test]
fn client_and_helper_agree_on_protocol() {
    let test_executable = std::env::current_exe().unwrap();
    let debug_dir = test_executable.parent().unwrap().parent().unwrap();
    let helper_binary = debug_dir.join("splitroute-helper");
    if !helper_binary.is_file() {
        eprintln!(
            "пропуск: {} не собран (нужен `cargo test --workspace`)",
            helper_binary.display()
        );
        return;
    }

    let dir = TempDir::new("roundtrip");
    let socket = dir.path.join("helper.sock");
    let runtime = dir.path.join("runtime");
    let sing_box = dir.path.join("sing-box");
    fs::write(&sing_box, OK_CORE).unwrap();
    fs::set_permissions(&sing_box, fs::Permissions::from_mode(0o755)).unwrap();

    let mut helper = Helper::spawn(&helper_binary, &socket, &runtime, &sing_box);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !socket.exists() {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(socket.exists(), "helper не создал сокет");

    let client = UnixHelperClient::new(socket.clone());

    let status = client.status().unwrap();
    assert_eq!(status.state, CoreState::Stopped);

    let started = client
        .start(json!({ "log": { "level": "info" } }), false)
        .unwrap();
    assert_eq!(started.state, CoreState::Running);
    assert!(started.pid.is_some());

    let after = client.status().unwrap();
    assert_eq!(after.state, CoreState::Running);
    assert_eq!(after.pid, started.pid);

    let stopped = client.stop().unwrap();
    assert_eq!(stopped.state, CoreState::Stopped);

    helper.terminate();
}
