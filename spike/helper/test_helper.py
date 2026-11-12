#!/usr/bin/env python3
"""S1-T10: тесты прототипа helper'а (dev-режим, без установки сервиса).

Проверяется: права сокета 0600, start/stop/status, запуск реального sing-box,
неизвестная команда, несовпадение версии протокола, невалидный JSON, лимит размера.

Запуск: python3 -m unittest spike/helper/test_helper.py -v
"""

from __future__ import annotations

import importlib.util
import json
import os
import socket
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HELPER = Path(__file__).resolve().parent / "helper.py"
ROOT = Path(__file__).resolve().parents[2]


def find_sing_box() -> Path:
    binaries = ROOT / "src-tauri" / "binaries"
    for candidate in sorted(binaries.glob("sing-box-*")):
        if candidate.is_file():
            return candidate
    raise unittest.SkipTest("pinned sing-box не найден: запустите scripts/fetch-core.sh")


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def load_helper_module():
    spec = importlib.util.spec_from_file_location("splitroute_helper", HELPER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class AllowedUidTest(unittest.TestCase):
    def test_sudo_uid_is_allowed_under_root(self) -> None:
        helper = load_helper_module()
        with mock.patch("os.geteuid", return_value=0), mock.patch.dict(
            os.environ, {"SUDO_UID": "501"}, clear=False
        ):
            self.assertEqual(helper.allowed_peer_uid(), 501)

    def test_missing_sudo_uid_falls_back_to_current_uid(self) -> None:
        helper = load_helper_module()
        with mock.patch("os.geteuid", return_value=0), mock.patch.dict(
            os.environ, {}, clear=True
        ):
            self.assertEqual(helper.allowed_peer_uid(), os.getuid())

    def test_non_root_uses_current_uid(self) -> None:
        helper = load_helper_module()
        with mock.patch("os.geteuid", return_value=os.getuid()):
            self.assertEqual(helper.allowed_peer_uid(), os.getuid())


class HelperTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sing_box = find_sing_box()
        cls.workdir = Path(tempfile.mkdtemp(prefix="splitroute-helper-test-")).resolve()
        cls.socket_path = cls.workdir / "helper.sock"
        cls.runtime_dir = cls.workdir / "runtime"
        cls.log_path = cls.workdir / "helper.log"
        cls.log_file = open(cls.log_path, "w", encoding="utf-8")  # noqa: SIM115 — закрывается в tearDownClass
        cls.process = subprocess.Popen(
            [
                sys.executable,
                str(HELPER),
                "--socket",
                str(cls.socket_path),
                "--runtime-dir",
                str(cls.runtime_dir),
                "--sing-box",
                str(cls.sing_box),
            ],
            stdout=cls.log_file,
            stderr=subprocess.STDOUT,
            text=True,
        )
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if cls.socket_path.exists():
                return
            time.sleep(0.1)
        raise RuntimeError(f"helper не создал сокет (лог: {cls.log_path})")

    @classmethod
    def tearDownClass(cls) -> None:
        cls.process.terminate()
        try:
            cls.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            cls.process.kill()
        cls.log_file.close()

    def call(self, request: dict | bytes) -> dict:
        payload = request if isinstance(request, bytes) else (
            json.dumps(request, ensure_ascii=False).encode() + b"\n"
        )
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(30)
            client.connect(str(self.socket_path))
            client.sendall(payload)
            buffer = b""
            while b"\n" not in buffer:
                chunk = client.recv(65536)
                if not chunk:
                    break
                buffer += chunk
        return json.loads(buffer.split(b"\n", 1)[0])

    def test_socket_permissions(self) -> None:
        mode = stat.S_IMODE(self.socket_path.stat().st_mode)
        self.assertEqual(mode, 0o600, f"права сокета: {oct(mode)}")

    def test_status_and_lifecycle(self) -> None:
        port = free_port()
        status = self.call({"protocol": 1, "id": "s1", "command": "status"})
        self.assertTrue(status["ok"])
        self.assertEqual(status["payload"]["state"], "stopped")

        config = {
            "log": {"level": "info"},
            "dns": {"servers": [{"tag": "local", "type": "local"}], "final": "local"},
            "inbounds": [
                {"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": port}
            ],
            "outbounds": [{"type": "direct", "tag": "direct"}],
            "route": {"final": "direct", "default_domain_resolver": {"server": "local"}},
        }
        started = self.call(
            {"protocol": 1, "id": "s2", "command": "start", "payload": {"config": config}}
        )
        self.assertTrue(started["ok"], started)
        self.assertEqual(started["payload"]["state"], "running")
        pid = started["payload"]["pid"]
        self.assertGreater(pid, 0)

        # Ядро реально слушает mixed-inbound.
        deadline = time.monotonic() + 10
        connected = False
        while time.monotonic() < deadline:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
                probe.settimeout(0.3)
                if probe.connect_ex(("127.0.0.1", port)) == 0:
                    connected = True
                    break
            time.sleep(0.1)
        self.assertTrue(connected, "sing-box не слушает mixed-inbound")

        status = self.call({"protocol": 1, "id": "s3", "command": "status"})
        self.assertEqual(status["payload"]["state"], "running")
        self.assertEqual(status["payload"]["pid"], pid)

        stopped = self.call({"protocol": 1, "id": "s4", "command": "stop"})
        self.assertTrue(stopped["ok"])
        self.assertEqual(stopped["payload"]["state"], "stopped")
        time.sleep(0.3)
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    def test_unknown_command(self) -> None:
        response = self.call({"protocol": 1, "id": "u1", "command": "exec"})
        self.assertFalse(response["ok"])
        self.assertEqual(response["error"]["code"], "UNKNOWN_COMMAND")
        self.assertEqual(response["id"], "u1")

    def test_protocol_mismatch(self) -> None:
        response = self.call({"protocol": 99, "id": "p1", "command": "status"})
        self.assertFalse(response["ok"])
        self.assertEqual(response["error"]["code"], "PROTOCOL_MISMATCH")

    def test_bad_json(self) -> None:
        response = self.call(b"not json\n")
        self.assertFalse(response["ok"])
        self.assertEqual(response["error"]["code"], "BAD_REQUEST")

    def test_message_too_large(self) -> None:
        oversized = b'{"protocol":1,"id":"big","command":"status","pad":"' + b"x" * (1024 * 1024 + 64)
        response = self.call(oversized)
        self.assertFalse(response["ok"])
        self.assertEqual(response["error"]["code"], "TOO_LARGE")


if __name__ == "__main__":
    unittest.main()
