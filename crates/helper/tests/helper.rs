//! Интеграционные тесты helper'а: реальный бинарь, Unix-сокет, фиктивное ядро.
//!
//! Настоящий sing-box не нужен: скрипт-заглушка отвечает на `version`, `check`
//! и `run`, поэтому тесты герметичны и не зависят от сети.

#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use splitroute_protocol::PROTOCOL_VERSION;

const BIN: &str = env!("CARGO_BIN_EXE_splitroute-helper");

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

const OK_CORE: &str = r#"#!/bin/sh
case "$1" in
  version) echo "sing-box version 1.14.4-test" ;;
  check) exit 0 ;;
  run)
    echo $$ > "$(dirname "$0")/run.pid"
    trap 'exit 0' TERM
    while :; do sleep 0.1; done
    ;;
  *) exit 1 ;;
esac
"#;

const FAILING_CORE: &str = r#"#!/bin/sh
case "$1" in
  version) echo "sing-box version 1.14.4-test" ;;
  *) exit 1 ;;
esac
"#;

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
        // Короткий путь: длина Unix-сокета ограничена.
        let path = PathBuf::from("/tmp").join(format!(
            "sr-helper-{tag}-{}-{sequence}-{unique}",
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

struct HelperProcess {
    child: Option<Child>,
    dir: TempDir,
    socket: PathBuf,
    runtime: PathBuf,
}

impl HelperProcess {
    fn spawn(sing_box: &Path, extra: &[&str]) -> Self {
        let dir = TempDir::new("proc");
        let socket = dir.path.join("helper.sock");
        let runtime = dir.path.join("runtime");
        let log = fs::File::create(dir.path.join("helper.log")).unwrap();

        let child = Command::new(BIN)
            .arg("--socket")
            .arg(&socket)
            .arg("--runtime-dir")
            .arg(&runtime)
            .arg("--sing-box")
            .arg(sing_box)
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .expect("запуск splitroute-helper");

        let process = Self {
            child: Some(child),
            dir,
            socket,
            runtime,
        };
        process.wait_for_socket();
        process
    }

    fn wait_for_socket(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.socket.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "helper не создал сокет; лог:\n{}",
            fs::read_to_string(self.dir.path.join("helper.log")).unwrap_or_default()
        );
    }

    fn socket_path(&self) -> &Path {
        &self.socket
    }

    fn runtime(&self) -> &Path {
        &self.runtime
    }

    fn pid(&self) -> u32 {
        self.child.as_ref().expect("процесс helper'а").id()
    }

    fn call(&self, request: &[u8]) -> Value {
        let mut stream = UnixStream::connect(&self.socket).expect("подключение к helper'у");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request).unwrap();
        if !request.ends_with(b"\n") {
            stream.write_all(b"\n").unwrap();
        }
        read_response(&stream)
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

impl Drop for HelperProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn read_response(stream: &UnixStream) -> Value {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    assert!(!line.is_empty(), "helper закрыл соединение без ответа");
    serde_json::from_str(&line).expect("ответ helper'а — валидный JSON")
}

fn request(id: &str, command: &str, payload: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "protocol": PROTOCOL_VERSION,
        "id": id,
        "command": command,
        "payload": payload,
    }))
    .unwrap()
}

fn status_request(id: &str) -> Vec<u8> {
    request(id, "status", Value::Null)
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("условие не выполнилось за 5 секунд");
}

#[test]
fn socket_is_private_and_status_is_stopped() {
    let dir = TempDir::new("status");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    assert_eq!(mode_of(helper.socket_path()), 0o600);

    let response = helper.call(&status_request("s1"));
    assert_eq!(response["ok"], true);
    assert_eq!(response["id"], "s1");
    assert_eq!(response["payload"]["state"], "stopped");
    assert_eq!(
        response["payload"]["version"],
        "sing-box version 1.14.4-test"
    );
}

#[test]
fn start_stop_lifecycle_is_idempotent() {
    let dir = TempDir::new("lifecycle");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let started = helper.call(&request(
        "start",
        "start",
        json!({ "config": { "log": { "level": "info" } } }),
    ));
    assert_eq!(started["ok"], true, "{started}");
    assert_eq!(started["payload"]["state"], "running");
    let pid = started["payload"]["pid"].as_u64().unwrap() as u32;
    assert!(pid > 0);
    assert!(started["payload"]["started_at"].is_number());

    let config_path = helper.runtime().join("config.json");
    assert_eq!(mode_of(&config_path), 0o600);
    let written: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(written["log"]["level"], "info");

    let run_pid: u32 = fs::read_to_string(dir.path.join("run.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(run_pid, pid);

    let again = helper.call(&request(
        "again",
        "start",
        json!({ "config": { "log": { "level": "info" } } }),
    ));
    assert_eq!(again["ok"], false);
    assert_eq!(again["error"]["code"], "ALREADY_RUNNING");

    let status = helper.call(&status_request("status"));
    assert_eq!(status["payload"]["state"], "running");
    assert_eq!(status["payload"]["pid"].as_u64().unwrap() as u32, pid);

    let stopped = helper.call(&request("stop", "stop", Value::Null));
    assert_eq!(stopped["ok"], true);
    assert_eq!(stopped["payload"]["state"], "stopped");
    wait_until(|| !process_alive(pid));
    assert!(!config_path.exists(), "конфиг удаляется после остановки");

    let stopped_again = helper.call(&request("stop2", "stop", Value::Null));
    assert_eq!(stopped_again["ok"], true);
    assert_eq!(stopped_again["payload"]["state"], "stopped");
}

#[test]
fn invalid_config_is_not_started() {
    let dir = TempDir::new("invalid");
    let core = script(&dir.path, "sing-box", FAILING_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let response = helper.call(&request(
        "start",
        "start",
        json!({ "config": { "log": { "level": "info" } } }),
    ));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "CONFIG_INVALID");
    assert!(!helper.runtime().join("config.json").exists());

    let status = helper.call(&status_request("status"));
    assert_eq!(status["payload"]["state"], "stopped");
}

#[test]
fn unknown_command_is_rejected() {
    let dir = TempDir::new("unknown");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let response = helper.call(&request("u1", "exec", Value::Null));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "UNKNOWN_COMMAND");
    assert_eq!(response["id"], "u1");
}

#[test]
fn install_and_uninstall_are_forbidden_over_socket() {
    let dir = TempDir::new("install");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    for command in ["install", "uninstall"] {
        let response = helper.call(&request("i1", command, Value::Null));
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "FORBIDDEN");
    }
}

#[test]
fn protocol_mismatch_is_rejected() {
    let dir = TempDir::new("protocol");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let raw = serde_json::to_vec(&json!({
        "protocol": 99,
        "id": "p1",
        "command": "status",
    }))
    .unwrap();
    let response = helper.call(&raw);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "PROTOCOL_MISMATCH");
    assert_eq!(response["id"], "p1");
}

#[test]
fn bad_json_is_rejected() {
    let dir = TempDir::new("badjson");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let response = helper.call(b"not json");
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "BAD_REQUEST");
}

#[test]
fn oversized_message_is_rejected() {
    let dir = TempDir::new("oversized");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let mut stream = UnixStream::connect(helper.socket_path()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let payload = vec![b'x'; 1024 * 1024 + 64];
    let _ = stream.write_all(&payload);
    let _ = stream.shutdown(Shutdown::Write);

    let response = read_response(&stream);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "TOO_LARGE");
}

#[test]
fn foreign_uid_is_forbidden() {
    let dir = TempDir::new("foreign");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let uid = unsafe { libc::getuid() };
    let foreign = if uid == 0 { 1 } else { uid + 1 };
    let foreign = foreign.to_string();
    let helper = HelperProcess::spawn(&core, &["--allow-uid", &foreign]);

    let response = helper.call(&status_request("f1"));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "FORBIDDEN");
}

#[test]
fn second_instance_does_not_disturb_running_helper() {
    let dir = TempDir::new("duplicate");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let helper = HelperProcess::spawn(&core, &[]);

    let started = helper.call(&request(
        "start",
        "start",
        json!({ "config": { "log": { "level": "info" } } }),
    ));
    assert_eq!(started["ok"], true, "{started}");
    let config_path = helper.runtime().join("config.json");
    assert!(config_path.exists());

    let mut second = Command::new(BIN)
        .arg("--socket")
        .arg(helper.socket_path())
        .arg("--runtime-dir")
        .arg(helper.runtime())
        .arg("--sing-box")
        .arg(&core)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = second.try_wait().unwrap() {
            assert!(
                !status.success(),
                "второй экземпляр должен завершиться ошибкой"
            );
            break;
        }
        if Instant::now() > deadline {
            let _ = second.kill();
            let _ = second.wait();
            panic!("второй экземпляр не завершился");
        }
        thread::sleep(Duration::from_millis(20));
    }

    assert!(
        config_path.exists(),
        "второй экземпляр не должен удалять конфиг работающего helper'а"
    );
    let status = helper.call(&status_request("status"));
    assert_eq!(status["payload"]["state"], "running");
}

#[test]
fn sigterm_stops_core_and_removes_socket() {
    let dir = TempDir::new("shutdown");
    let core = script(&dir.path, "sing-box", OK_CORE);
    let mut helper = HelperProcess::spawn(&core, &[]);

    let started = helper.call(&request(
        "start",
        "start",
        json!({ "config": { "log": { "level": "info" } } }),
    ));
    assert_eq!(started["ok"], true, "{started}");
    let core_pid = started["payload"]["pid"].as_u64().unwrap() as u32;
    assert!(helper.pid() > 0);

    helper.terminate();
    wait_until(|| !process_alive(core_pid));
    assert!(
        !helper.socket_path().exists(),
        "сокет удаляется при завершении helper'а"
    );
}
