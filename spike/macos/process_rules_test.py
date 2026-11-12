#!/usr/bin/env python3
"""S1-T4: проверка process-правил sing-box на macOS без root (без TUN).

Зачем без TUN
-------------
TUN требует root, а в сессии спайка passwordless sudo нет. Но сами process-правила
(`process_name`, `process_path_regex`) ядро резолвит по локальному сокету-источнику,
поэтому их можно проверить через mixed-inbound на 127.0.0.1: клиент подключается
к SOCKS5-порту sing-box, а route-правило решает, в какой outbound его отправить.

Чтобы результат был наблюдаемым без реального сервера, оба outbound'а — `socks`
на два разных локальных «фейковых» SOCKS5-сервера, которые на любой CONNECT
отвечают HTTP 200 с разными маркерами:

    outbound "proxy"  -> 127.0.0.1:<marker>  -> тело "MARKER-PROXY"
    outbound "direct" -> 127.0.0.1:<target>  -> тело "MARKER-DIRECT"

Клиент запрашивает публичный адрес TEST-NET-1 (192.0.2.10), поэтому правило
`ip_is_private` из боевого конфига не вмешивается и трафик в интернет не уходит.
`override_address` не используется: в sing-box 1.13+ он удалён из direct-outbound.

Что проверяется
---------------
1. process_path_regex: `socksprobe` по разрешённому пути идёт в "proxy",
   тот же бинарь по другому пути — в "direct".
2. process_name: `socksprobe` идёт в "proxy", `socksprobe-b` — в "direct".
3. (опционально) process_path_regex Chrome: headless Chrome идёт в "proxy".

Это не заменяет TUN-тест: маршрутизация TUN, DNS через TUN и UDP здесь не
проверяются. TUN-часть фиксируется в README как «не проверено» с причиной.

Использование:
    python3 spike/macos/process_rules_test.py            # все доступные кейсы
    python3 spike/macos/process_rules_test.py --no-chrome
Коды возврата: 0 — все выполненные кейсы совпали с ожиданием, 1 — расхождение.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path

MARKER_PROXY = "MARKER-PROXY"
MARKER_DIRECT = "MARKER-DIRECT"
# Публичный адрес из TEST-NET-1: гарантирует, что ip_is_private не сработает.
DESTINATION = "192.0.2.10"
CHROME = Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")


@dataclass
class Case:
    """Один прогон: набор route-правил и пары (метка, ожидаемый маркер)."""

    name: str
    rules: list[dict]
    invocations: list[tuple[str, str]]


class FakeSocksServer:
    """Минимальный SOCKS5-сервер: на любой CONNECT отвечает HTTP 200 с маркером.

    Нужен только для наблюдаемости: реальный прокси не требуется, а соединение
    в интернет не выполняется.
    """

    def __init__(self, body: str) -> None:
        self._body = body
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._socket.bind(("127.0.0.1", 0))
        self._socket.listen(16)
        self.port = self._socket.getsockname()[1]
        self._closed = False
        threading.Thread(target=self._accept_loop, daemon=True).start()

    def _accept_loop(self) -> None:
        while not self._closed:
            try:
                client, _ = self._socket.accept()
            except OSError:
                return
            threading.Thread(target=self._handle, args=(client,), daemon=True).start()

    def _handle(self, client: socket.socket) -> None:
        try:
            client.settimeout(15)
            greeting = client.recv(2)
            if len(greeting) < 2 or greeting[0] != 0x05:
                return
            client.recv(greeting[1])  # методы аутентификации; отвечаем «без пароля»
            client.sendall(b"\x05\x00")
            header = client.recv(4)
            if len(header) < 4 or header[1] != 0x01:  # только CONNECT
                return
            atyp = header[3]
            if atyp == 0x01:
                client.recv(4)
            elif atyp == 0x03:
                client.recv(client.recv(1)[0])
            elif atyp == 0x04:
                client.recv(16)
            client.recv(2)  # порт
            client.sendall(b"\x05\x00\x00\x01\x00\x00\x00\x00\x00\x00")
            # Читаем HTTP-запрос (до пустой строки), затем отдаём ответ.
            buffer = b""
            while b"\r\n\r\n" not in buffer and len(buffer) < 65536:
                chunk = client.recv(4096)
                if not chunk:
                    break
                buffer += chunk
            payload = self._body.encode()
            client.sendall(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n"
                + f"Content-Length: {len(payload)}\r\n".encode()
                + b"Connection: close\r\n\r\n"
                + payload
            )
        except OSError:
            pass
        finally:
            try:
                client.close()
            except OSError:
                pass

    def close(self) -> None:
        self._closed = True
        try:
            self._socket.close()
        except OSError:
            pass


def free_port() -> int:
    """Возвращает свободный TCP-порт на loopback."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_port(port: int, timeout: float = 10.0) -> bool:
    """Ждёт, пока sing-box начнёт принимать соединения на mixed-порту."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.3)
            if sock.connect_ex(("127.0.0.1", port)) == 0:
                return True
        time.sleep(0.1)
    return False


def find_sing_box(explicit: str | None) -> Path:
    """Ищет pinned-бинарь в src-tauri/binaries, затем в PATH."""
    if explicit:
        return Path(explicit)
    binaries = Path(__file__).resolve().parents[2] / "src-tauri" / "binaries"
    for candidate in sorted(binaries.glob("sing-box-*")):
        if candidate.is_file():
            return candidate
    found = shutil.which("sing-box")
    if not found:
        raise SystemExit("ошибка: sing-box не найден, укажите --sing-box")
    return Path(found)


def build_config(mixed_port: int, marker_port: int, target_port: int, rules: list[dict]) -> dict:
    """Конфиг без TUN: mixed-inbound + два socks-outbound на фейковые серверы."""
    return {
        "log": {"level": "debug", "timestamp": True},
        "dns": {
            "servers": [{"tag": "local", "type": "local"}],
            "final": "local",
        },
        "inbounds": [
            {
                "type": "mixed",
                "tag": "mixed-in",
                "listen": "127.0.0.1",
                "listen_port": mixed_port,
            }
        ],
        "outbounds": [
            # Наблюдаемая замена реального proxy.
            {"type": "socks", "tag": "proxy", "server": "127.0.0.1", "server_port": marker_port},
            # Наблюдаемая замена «интернета» для невыбранных процессов.
            {"type": "socks", "tag": "direct", "server": "127.0.0.1", "server_port": target_port},
        ],
        "route": {
            "rules": rules,
            "final": "direct",
            "default_domain_resolver": {"server": "local"},
        },
    }


def run_sing_box(binary: Path, config: dict, workdir: Path) -> tuple[subprocess.Popen, Path]:
    """Запускает sing-box и возвращает процесс и путь к логу."""
    workdir.mkdir(parents=True, exist_ok=True)
    config_path = workdir / "config.json"
    config_path.write_text(json.dumps(config, ensure_ascii=False, indent=2), encoding="utf-8")
    log_path = workdir / "sing-box.log"
    log_file = open(log_path, "w", encoding="utf-8")  # noqa: SIM115 — закрывается в stop
    process = subprocess.Popen(
        [str(binary), "run", "-c", str(config_path)],
        stdout=log_file,
        stderr=subprocess.STDOUT,
        text=True,
    )
    process._splitroute_log_file = log_file  # type: ignore[attr-defined]
    return process, log_path


def stop_sing_box(process: subprocess.Popen) -> None:
    """Останавливает sing-box и закрывает лог."""
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    log_file = getattr(process, "_splitroute_log_file", None)
    if log_file is not None:
        log_file.close()


def request_via_socks(binary: Path, mixed_port: int, url: str) -> str:
    """Выполняет запрос тестируемым бинарём через mixed-inbound (SOCKS5)."""
    result = subprocess.run(
        [
            str(binary),
            "-proxy",
            f"127.0.0.1:{mixed_port}",
            "-url",
            url,
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return f"ERROR: {result.stderr.strip() or result.returncode}"
    return result.stdout.strip()


def build_probe(workdir: Path) -> Path:
    """Собирает socksprobe.go (нужен Go) — свой бинарь можно копировать и переименовывать."""
    source = Path(__file__).resolve().parent / "socksprobe.go"
    target = workdir / "probe-bin" / "socksprobe"
    target.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        ["go", "build", "-o", str(target), str(source)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"ошибка сборки socksprobe (нужен Go): {result.stderr.strip()}")
    return target


def run_case(
    binary: Path,
    case: Case,
    workdir: Path,
    marker_port: int,
    target_port: int,
) -> list[tuple[str, str, str, bool]]:
    """Прогоняет один кейс. Возвращает строки: (кейс, метка, факт, ok)."""
    mixed_port = free_port()
    config = build_config(mixed_port, marker_port, target_port, case.rules)
    process, log_path = run_sing_box(binary, config, workdir / case.name)
    results: list[tuple[str, str, str, bool]] = []
    try:
        if not wait_for_port(mixed_port):
            raise SystemExit(f"ошибка: sing-box не поднял mixed-inbound (лог: {log_path})")
        for label, expected in case.invocations:
            try:
                actual = request_via_socks(Path(label), mixed_port, f"http://{DESTINATION}/")
            except Exception as error:  # noqa: BLE001 — результат важнее типа ошибки
                actual = f"ERROR: {error}"
            results.append((case.name, label, actual, actual == expected))
    finally:
        stop_sing_box(process)
    return results


def chrome_case(
    binary: Path, workdir: Path, marker_port: int, target_port: int
) -> list[tuple[str, str, str, bool]]:
    """Опционально: headless Chrome через process_path_regex по каталогу .app."""
    if not CHROME.is_file():
        return []
    mixed_port = free_port()
    rules = [{"process_path_regex": [r"^/Applications/Google Chrome\.app/"], "outbound": "proxy"}]
    config = build_config(mixed_port, marker_port, target_port, rules)
    case_dir = workdir / "chrome"
    process, log_path = run_sing_box(binary, config, case_dir)
    try:
        if not wait_for_port(mixed_port):
            raise SystemExit(f"ошибка: sing-box не поднял mixed-inbound (лог: {log_path})")
        argv = [
            str(CHROME),
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            f"--user-data-dir={case_dir / 'profile'}",
            f"--proxy-server=http://127.0.0.1:{mixed_port}",
            "--dump-dom",
            f"http://{DESTINATION}/",
        ]
        try:
            completed = subprocess.run(argv, capture_output=True, text=True, timeout=45)
        except subprocess.TimeoutExpired:
            return [("chrome", "headless --dump-dom", "ERROR: timeout", False)]
        out = completed.stdout
        if MARKER_PROXY in out:
            actual = MARKER_PROXY
        elif MARKER_DIRECT in out:
            actual = MARKER_DIRECT
        else:
            actual = (
                f"UNEXPECTED: rc={completed.returncode} out={out[:80]!r} "
                f"err={completed.stderr[-160:]!r}"
            )
        return [("chrome", "headless --dump-dom", actual, actual == MARKER_PROXY)]
    finally:
        stop_sing_box(process)


def platform_version() -> str:
    """Версия macOS из sw_vers."""
    return subprocess.run(["sw_vers", "-productVersion"], capture_output=True, text=True).stdout.strip()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Проверка process-правил sing-box на macOS")
    parser.add_argument("--sing-box", default=None, help="путь к бинарю sing-box")
    parser.add_argument("--no-chrome", action="store_true", help="не проверять Chrome")
    parser.add_argument("--out", default=None, help="куда сохранить сводку вместо stdout")
    args = parser.parse_args(argv)

    binary = find_sing_box(args.sing_box)
    print(f"sing-box: {binary}")
    version = subprocess.run([str(binary), "version"], capture_output=True, text=True)
    print(version.stdout.strip().splitlines()[0] if version.stdout else "версия неизвестна")
    print(f"macOS: {platform_version()}  arch: {os.uname().machine}")

    marker_server = FakeSocksServer(MARKER_PROXY)
    target_server = FakeSocksServer(MARKER_DIRECT)
    # resolve(): на macOS /var — симлинк на /private/var, а sing-box сообщает
    # канонический путь процесса. Иначе process_path_regex не совпадёт.
    workdir = Path(tempfile.mkdtemp(prefix="splitroute-s1t4-")).resolve()
    print(f"рабочий каталог (временный): {workdir}")

    probe = build_probe(workdir)
    path_a = workdir / "path-a" / "socksprobe"
    path_b = workdir / "path-b" / "socksprobe"
    named = workdir / "named" / "socksprobe"
    named_other = workdir / "named" / "socksprobe-b"
    for target in (path_a, path_b, named, named_other):
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(probe, target)
        target.chmod(0o755)

    cases = [
        Case(
            name="path_regex",
            rules=[{"process_path_regex": [f"^{re.escape(str(path_a))}$"], "outbound": "proxy"}],
            invocations=[
                (str(path_a), MARKER_PROXY),
                (str(path_b), MARKER_DIRECT),
            ],
        ),
        Case(
            name="process_name",
            rules=[{"process_name": ["socksprobe"], "outbound": "proxy"}],
            invocations=[
                (str(named), MARKER_PROXY),
                (str(named_other), MARKER_DIRECT),
            ],
        ),
    ]

    results: list[tuple[str, str, str, bool]] = []
    try:
        for case in cases:
            results.extend(run_case(binary, case, workdir, marker_server.port, target_server.port))
        if not args.no_chrome:
            try:
                results.extend(
                    chrome_case(binary, workdir, marker_server.port, target_server.port)
                )
            except SystemExit as error:
                results.append(("chrome", "headless --dump-dom", f"SKIP: {error}", False))
    finally:
        marker_server.close()
        target_server.close()

    lines = ["кейс\tзапрос\tфакт\tok"]
    for case_name, label, actual, ok in results:
        lines.append(f"{case_name}\t{label}\t{actual}\t{'ok' if ok else 'FAIL'}")
    report = "\n".join(lines)
    print(report)
    if args.out:
        Path(args.out).write_text(report + "\n", encoding="utf-8")

    return 0 if all(row[3] for row in results) else 1


if __name__ == "__main__":
    sys.exit(main())
