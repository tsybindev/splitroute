#!/usr/bin/env python3
"""S1-T8: проверка разделения DNS по процессу (без TUN).

Методика
--------
Два фейковых UDP-DNS-сервера на loopback записывают, какие имена у них спросили,
и отвечают A=127.0.0.1. В sing-box:

    dns.rules: process_name["socksprobe"] -> server "remote"
    dns.final: "local"

Запрос идёт через mixed-inbound на домен `dns-*.invalid`; route-правило
`action: resolve` заставляет ядро реально сходить в DNS до соединения. Смотрим,
на какой из двух серверов пришёл запрос:

    выбранный процесс  -> remote
    невыбранный процесс -> local

Что доказывает: DNS-правила с process-матчингом принимаются и применяются ядром.
Чего НЕ доказывает: перехват DNS через TUN, поведение systemd-resolved и Private
Relay — для них нужны root/Linux/актуальная macOS. Это фиксируется как
непроверенное в отчёте; PRD §6.4 требует fallback на системный DNS при провале.

Использование: python3 spike/macos/dns_rules_test.py
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
import process_rules_test as harness  # noqa: E402 — общие функции стенда

MARKER_PROXY = "MARKER-PROXY"
SELECTED_DOMAIN = "dns-selected.invalid"
OTHER_DOMAIN = "dns-other.invalid"


class FakeDns:
    """Минимальный UDP-DNS: запоминает имена и отвечает A=127.0.0.1."""

    def __init__(self) -> None:
        self.queries: list[str] = []
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self._socket.bind(("127.0.0.1", 0))
        self.port = self._socket.getsockname()[1]
        self._closed = False
        threading.Thread(target=self._loop, daemon=True).start()

    def _loop(self) -> None:
        while not self._closed:
            try:
                data, address = self._socket.recvfrom(4096)
            except OSError:
                return
            name, question_end = parse_question(data)
            self.queries.append(name)
            response = build_response(data, question_end)
            self._socket.sendto(response, address)

    def close(self) -> None:
        self._closed = True
        self._socket.close()


def parse_question(data: bytes) -> tuple[str, int]:
    """Возвращает имя из вопроса и смещение сразу после QTYPE/QCLASS."""
    offset = 12
    labels: list[str] = []
    while offset < len(data) and data[offset] != 0:
        length = data[offset]
        labels.append(data[offset + 1 : offset + 1 + length].decode(errors="replace"))
        offset += 1 + length
    offset += 1  # завершающий ноль
    return ".".join(labels), offset + 4  # + QTYPE/QCLASS


def build_response(data: bytes, question_end: int) -> bytes:
    """Ответ: эхо вопроса + один A-рекорд 127.0.0.1."""
    header = data[:2] + b"\x81\x80" + b"\x00\x01" + b"\x00\x01" + b"\x00\x00" + b"\x00\x00"
    question = data[12:question_end]
    answer = b"\xc0\x0c" + b"\x00\x01" + b"\x00\x01" + b"\x00\x00\x00\x3c" + b"\x00\x04" + bytes([127, 0, 0, 1])
    return header + question + answer


def build_config(mixed_port: int, remote_port: int, local_port: int, marker_port: int) -> dict:
    return {
        "log": {"level": "debug", "timestamp": True},
        "dns": {
            "servers": [
                {"tag": "remote", "type": "udp", "server": "127.0.0.1", "server_port": remote_port},
                {"tag": "local", "type": "udp", "server": "127.0.0.1", "server_port": local_port},
            ],
            "rules": [{"process_name": ["socksprobe"], "server": "remote"}],
            "final": "local",
        },
        "inbounds": [
            {"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": mixed_port}
        ],
        "outbounds": [
            {"type": "socks", "tag": "proxy", "server": "127.0.0.1", "server_port": marker_port}
        ],
        "route": {
            "rules": [
                {
                    "domain": [SELECTED_DOMAIN, OTHER_DOMAIN],
                    "action": "resolve",
                    "strategy": "ipv4_only",
                }
            ],
            "final": "proxy",
            "default_domain_resolver": {"server": "local"},
        },
    }


def request(binary: Path, mixed_port: int, domain: str) -> str:
    """Выполняет запрос тестируемым бинарём через SOCKS5 mixed-inbound."""
    result = subprocess.run(
        [str(binary), "-proxy", f"127.0.0.1:{mixed_port}", "-url", f"http://{domain}/"],
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

    remote_dns = FakeDns()
    local_dns = FakeDns()
    marker = harness.FakeSocksServer(MARKER_PROXY)
    workdir = Path(tempfile.mkdtemp(prefix="splitroute-s1t8-")).resolve()
    print(f"рабочий каталог (временный): {workdir}")

    probe = harness.build_probe(workdir)
    selected = workdir / "selected" / "socksprobe"
    other = workdir / "other" / "socksprobe-b"
    for target in (selected, other):
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(probe.read_bytes())
        target.chmod(0o755)

    mixed_port = harness.free_port()
    config = build_config(mixed_port, remote_dns.port, local_dns.port, marker.port)
    process, log_path = harness.run_sing_box(binary, config, workdir / "run")
    results: list[tuple[str, str, str, bool]] = []
    try:
        if not harness.wait_for_port(mixed_port):
            raise SystemExit(f"ошибка: sing-box не поднял mixed-inbound (лог: {log_path})")

        body = request(selected, mixed_port, SELECTED_DOMAIN)
        results.append(
            (
                "selected",
                f"body={body}; remote={remote_dns.queries} local={local_dns.queries}",
                "remote получил запрос, local — нет",
                SELECTED_DOMAIN in remote_dns.queries and SELECTED_DOMAIN not in local_dns.queries,
            )
        )

        remote_dns.queries.clear()
        local_dns.queries.clear()
        body = request(other, mixed_port, OTHER_DOMAIN)
        results.append(
            (
                "other",
                f"body={body}; remote={remote_dns.queries} local={local_dns.queries}",
                "local получил запрос, remote — нет",
                OTHER_DOMAIN in local_dns.queries and OTHER_DOMAIN not in remote_dns.queries,
            )
        )
    finally:
        harness.stop_sing_box(process)
        remote_dns.close()
        local_dns.close()
        marker.close()

    lines = ["кейс\tподробности\tожидание\tok"]
    for case_name, details, expected, ok in results:
        lines.append(f"{case_name}\t{details}\t{expected}\t{'ok' if ok else 'FAIL'}")
    print("\n".join(lines))
    return 0 if all(row[3] for row in results) else 1


if __name__ == "__main__":
    sys.exit(main())
