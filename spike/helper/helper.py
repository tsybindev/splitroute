#!/usr/bin/env python3
"""S1-T10: прототип привилегированного helper'а (dev-режим, без установки сервиса).

Назначение
----------
Проверить протокол и механику будущего helper'а: Unix-сокет с правами 0600,
проверка uid клиента, фиксированный набор команд, проверка конфига через
`sing-box check`, запуск/остановка ядра, идемпотентный `stop`.

Это СПАЙК, а не продакшн. Продакшн-helper будет на Rust (AGENTS.md §2, §3) и
получит права root через LaunchDaemon/systemd. Здесь Python выбран потому, что
в окружении спайка нет Rust-тулчейна. Файлы для установки сервиса лежат в
`deploy/`, но в этой сессии НЕ устанавливаются.

Безопасность (что уже реализовано)
----------------------------------
- сокет: AF_UNIX, права 0600, владелец — текущий пользователь;
- проверка uid пира: SO_PEERCRED (Linux) / getpeereid (macOS);
- команды — только `start`, `stop`, `status`; ничего больше;
- helper не принимает произвольные пути и не запускает shell: только объект
  конфига, который сам пишет в runtime-каталог с правами 0600;
- конфиг проверяется `sing-box check` до запуска;
- лимит размера сообщения (1 МиБ) и таймаут на чтение;
- версия протокола проверяется, несовпадение — ошибка;
- содержимое конфига не логируется (там UUID/ключи).

Использование (dev-режим)
-------------------------
    python3 spike/helper/helper.py \
        --socket "$TMPDIR/splitroute-helper.sock" \
        --runtime-dir "$TMPDIR/splitroute-helper" \
        --sing-box ./src-tauri/binaries/sing-box-aarch64-apple-darwin

Протокол — по одному JSON на строку, ответ тоже одной строкой.
"""

from __future__ import annotations

import argparse
import ctypes
import ctypes.util
import json
import os
import signal
import socket
import stat
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

PROTOCOL_VERSION = 1
MAX_MESSAGE_BYTES = 1024 * 1024
READ_TIMEOUT_SECONDS = 10
ALLOWED_COMMANDS = {"start", "stop", "status"}


class HelperError(Exception):
    """Ошибка протокола с кодом для клиента."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class Helper:
    """Состояние helper'а: один процесс sing-box и один runtime-каталог."""

    def __init__(self, sing_box: Path, runtime_dir: Path) -> None:
        self._sing_box = sing_box
        self._runtime_dir = runtime_dir
        self._runtime_dir.mkdir(parents=True, exist_ok=True)
        self._config_path = self._runtime_dir / "config.json"
        self._log_path = self._runtime_dir / "sing-box.log"
        self._process: subprocess.Popen | None = None
        self._started_at: float | None = None

    def handle(self, request: dict[str, Any]) -> dict[str, Any]:
        if request.get("protocol") != PROTOCOL_VERSION:
            raise HelperError(
                "PROTOCOL_MISMATCH",
                f"ожидается протокол {PROTOCOL_VERSION}, получен {request.get('protocol')!r}",
            )
        command = request.get("command")
        if command not in ALLOWED_COMMANDS:
            raise HelperError("UNKNOWN_COMMAND", f"команда {command!r} не разрешена")

        if command == "status":
            return self._status()
        if command == "start":
            return self._start(request.get("payload") or {})
        return self._stop()

    def _status(self) -> dict[str, Any]:
        running = self._process is not None and self._process.poll() is None
        if not running:
            self._process = None
        return {
            "state": "running" if running else "stopped",
            "pid": self._process.pid if running else None,
            "version": self._core_version(),
            "started_at": self._started_at if running else None,
        }

    def _start(self, payload: dict[str, Any]) -> dict[str, Any]:
        if self._process is not None and self._process.poll() is None:
            raise HelperError("ALREADY_RUNNING", "ядро уже запущено")

        config = payload.get("config")
        if not isinstance(config, dict):
            raise HelperError("CONFIG_INVALID", "payload.config должен быть объектом")

        # Конфиг пишется только в runtime-каталог helper'а, права 0600.
        self._config_path.write_text(
            json.dumps(config, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        os.chmod(self._config_path, 0o600)

        check = subprocess.run(
            [str(self._sing_box), "check", "-c", str(self._config_path)],
            capture_output=True,
            text=True,
        )
        if check.returncode != 0:
            self._config_path.unlink(missing_ok=True)
            # Текст ошибки ядра может содержать значения конфига — не отдаём его целиком.
            raise HelperError("CONFIG_INVALID", "sing-box check не прошёл")

        log_file = open(self._log_path, "a", encoding="utf-8")
        process = subprocess.Popen(
            [str(self._sing_box), "run", "-c", str(self._config_path)],
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
        )
        time.sleep(0.3)
        if process.poll() is not None:
            log_file.close()
            self._config_path.unlink(missing_ok=True)
            raise HelperError("START_FAILED", "ядро завершилось сразу после запуска")

        self._process = process
        self._started_at = time.time()
        return {
            "state": "running",
            "pid": process.pid,
            "version": self._core_version(),
            "started_at": self._started_at,
        }

    def _stop(self) -> dict[str, Any]:
        process = self._process
        self._process = None
        self._started_at = None
        if process is not None and process.poll() is None:
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        self._config_path.unlink(missing_ok=True)
        return {"state": "stopped"}

    def _core_version(self) -> str | None:
        try:
            result = subprocess.run(
                [str(self._sing_box), "version"], capture_output=True, text=True, timeout=5
            )
        except (OSError, subprocess.TimeoutExpired):
            return None
        first = result.stdout.strip().splitlines()
        return first[0] if first else None


def peer_uid(connection: socket.socket) -> int:
    """uid клиента сокета: getpeereid на macOS, SO_PEERCRED на Linux."""
    descriptor = connection.fileno()
    if sys.platform == "darwin":
        libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
        uid = ctypes.c_uint32()
        gid = ctypes.c_uint32()
        if libc.getpeereid(descriptor, ctypes.byref(uid), ctypes.byref(gid)) != 0:
            raise OSError(ctypes.get_errno(), "getpeereid failed")
        return uid.value

    import struct

    credentials = connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12)
    pid, uid, gid = struct.unpack("3i", credentials)
    return uid


def read_message(connection: socket.socket) -> bytes:
    """Читает одну строку JSON с лимитом размера."""
    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = connection.recv(65536)
        if not chunk:
            break
        chunks.append(chunk)
        total += len(chunk)
        if total > MAX_MESSAGE_BYTES:
            raise HelperError("TOO_LARGE", "сообщение превышает лимит")
        if b"\n" in chunk:
            break
    return b"".join(chunks).split(b"\n", 1)[0]


def serve_connection(connection: socket.socket, helper: Helper) -> None:
    connection.settimeout(READ_TIMEOUT_SECONDS)
    request_id: Any = None
    try:
        if peer_uid(connection) != os.getuid():
            raise HelperError("FORBIDDEN", "клиент с другим uid")
        raw = read_message(connection)
        if not raw.strip():
            raise HelperError("BAD_REQUEST", "пустое сообщение")
        try:
            request = json.loads(raw)
        except json.JSONDecodeError:
            raise HelperError("BAD_REQUEST", "невалидный JSON") from None
        if not isinstance(request, dict):
            raise HelperError("BAD_REQUEST", "ожидался JSON-объект")
        request_id = request.get("id")
        payload = helper.handle(request)
        response = {"protocol": PROTOCOL_VERSION, "id": request_id, "ok": True, "payload": payload}
    except HelperError as error:
        response = {
            "protocol": PROTOCOL_VERSION,
            "id": request_id,
            "ok": False,
            "error": {"code": error.code, "message": error.message},
        }
    except (OSError, ValueError) as error:
        response = {
            "protocol": PROTOCOL_VERSION,
            "id": request_id,
            "ok": False,
            "error": {"code": "INTERNAL", "message": str(error)},
        }
    try:
        connection.sendall(json.dumps(response, ensure_ascii=False).encode() + b"\n")
    except OSError:
        pass


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Прототип helper'а SplitRoute (spike)")
    parser.add_argument("--socket", required=True, help="путь к Unix-сокету")
    parser.add_argument("--runtime-dir", required=True, help="каталог конфига и логов")
    parser.add_argument("--sing-box", required=True, help="путь к бинарю sing-box")
    args = parser.parse_args(argv)

    sing_box = Path(args.sing_box).resolve()
    if not sing_box.is_file():
        print(f"ошибка: sing-box не найден: {sing_box}", file=sys.stderr)
        return 1

    socket_path = Path(args.socket)
    socket_path.parent.mkdir(parents=True, exist_ok=True)
    socket_path.unlink(missing_ok=True)

    helper = Helper(sing_box, Path(args.runtime_dir).resolve())
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(socket_path))
    os.chmod(socket_path, stat.S_IRUSR | stat.S_IWUSR)  # 0600
    server.listen(8)
    print(f"helper слушает {socket_path} (0600)", file=sys.stderr)

    try:
        while True:
            connection, _ = server.accept()
            with connection:
                serve_connection(connection, helper)
    except KeyboardInterrupt:
        pass
    finally:
        helper._stop()  # noqa: SLF001 — останавливаем ядро при выходе
        server.close()
        socket_path.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
