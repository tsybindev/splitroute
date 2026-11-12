#!/usr/bin/env python3
"""S1-T9: проверка UDP/QUIC-трафика и process-правил для UDP.

Методика
--------
TUN без root не поднимается, поэтому UDP проверяется через mixed-inbound и
SOCKS5 UDP ASSOCIATE (его реализует `socksprobe -udp`). Маршрут различается
правилами route с `override_address`/`override_port` (в sing-box 1.13+ это замена
удалённым override-полям outbound):

    process_name["socksprobe"] -> UDP-эхо "UDP-PROXY"
    остальные (ip_cidr 0.0.0.0/0) -> UDP-эхо "UDP-DIRECT"

Назначение — публичный TEST-NET-1 (192.0.2.10), реального трафика нет.

Что это доказывает: применяются ли process-правила к UDP и проходит ли UDP через
ядро. Чего это НЕ доказывает: поведение QUIC поверх реального TUN и реального
VLESS-сервера — это фиксируется как непроверенное.

Использование: python3 spike/macos/udp_rules_test.py
"""

from __future__ import annotations

import json
import socket
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import process_rules_test as harness  # noqa: E402 — переиспользуем общие функции стенда

UDP_PROXY = "UDP-PROXY"
UDP_DIRECT = "UDP-DIRECT"
DESTINATION_HOST = "192.0.2.10"
DESTINATION_PORT = 9999


class UdpEcho:
    """UDP-эхо на loopback: на любой пакет отвечает своим маркером."""

    def __init__(self, marker: str) -> None:
        self._marker = marker.encode()
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self._socket.bind(("127.0.0.1", 0))
        self.port = self._socket.getsockname()[1]
        self._closed = False
        threading.Thread(target=self._loop, daemon=True).start()

    def _loop(self) -> None:
        while not self._closed:
            try:
                _, address = self._socket.recvfrom(4096)
                self._socket.sendto(self._marker, address)
            except OSError:
                return

    def close(self) -> None:
        self._closed = True
        self._socket.close()


def build_config(mixed_port: int, proxy_port: int, direct_port: int) -> dict:
    """Конфиг с UDP-маршрутизацией по процессу через route override."""
    return {
        "log": {"level": "debug", "timestamp": True},
        "dns": {"servers": [{"tag": "local", "type": "local"}], "final": "local"},
        "inbounds": [
            {"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": mixed_port}
        ],
        "outbounds": [{"type": "direct", "tag": "direct"}],
        "route": {
            "rules": [
                {
                    "process_name": ["socksprobe"],
                    "action": "route",
                    "outbound": "direct",
                    "override_address": "127.0.0.1",
                    "override_port": proxy_port,
                },
                {
                    "ip_cidr": ["0.0.0.0/0"],
                    "action": "route",
                    "outbound": "direct",
                    "override_address": "127.0.0.1",
                    "override_port": direct_port,
                },
            ],
            "final": "direct",
            "default_domain_resolver": {"server": "local"},
        },
    }


def run_probe(binary: Path, mixed_port: int) -> str:
    """Отправляет UDP через SOCKS5 UDP ASSOCIATE тестируемым бинарём."""
    result = subprocess.run(
        [
            str(binary),
            "-udp",
            "-proxy",
            f"127.0.0.1:{mixed_port}",
            "-url",
            f"http://{DESTINATION_HOST}:{DESTINATION_PORT}/",
        ],
        capture_output=True,
        text=True,
        timeout=20,
    )
    if result.returncode != 0:
        return f"ERROR: {result.stderr.strip() or result.returncode}"
    return result.stdout.strip()


def main() -> int:
    binary = harness.find_sing_box(None)
    print(f"sing-box: {binary}")
    version = subprocess.run([str(binary), "version"], capture_output=True, text=True)
    print(version.stdout.strip().splitlines()[0])
    print(f"macOS: {harness.platform_version()}  arch: {__import__('os').uname().machine}")

    proxy_echo = UdpEcho(UDP_PROXY)
    direct_echo = UdpEcho(UDP_DIRECT)
    workdir = Path(tempfile.mkdtemp(prefix="splitroute-s1t9-")).resolve()
    print(f"рабочий каталог (временный): {workdir}")

    probe = harness.build_probe(workdir)
    selected = workdir / "selected" / "socksprobe"
    other = workdir / "other" / "socksprobe-b"
    for target in (selected, other):
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(probe.read_bytes())
        target.chmod(0o755)

    mixed_port = harness.free_port()
    config = build_config(mixed_port, proxy_echo.port, direct_echo.port)
    process, log_path = harness.run_sing_box(binary, config, workdir / "run")
    results: list[tuple[str, str, str, bool]] = []
    try:
        if not harness.wait_for_port(mixed_port):
            raise SystemExit(f"ошибка: sing-box не поднял mixed-inbound (лог: {log_path})")
        for label, expected in ((str(selected), UDP_PROXY), (str(other), UDP_DIRECT)):
            try:
                actual = run_probe(Path(label), mixed_port)
            except Exception as error:  # noqa: BLE001
                actual = f"ERROR: {error}"
            results.append(("udp", label, actual, actual == expected))
    finally:
        harness.stop_sing_box(process)
        proxy_echo.close()
        direct_echo.close()

    lines = ["кейс\tзапрос\tфакт\tok"]
    for case_name, label, actual, ok in results:
        lines.append(f"{case_name}\t{label}\t{actual!r}\t{'ok' if ok else 'FAIL'}")
    print("\n".join(lines))
    return 0 if all(row[3] for row in results) else 1


if __name__ == "__main__":
    sys.exit(main())
