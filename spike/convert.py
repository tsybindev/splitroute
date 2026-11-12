#!/usr/bin/env python3
"""Конвертация Happ-совместимой подписки (массив Xray-конфигов) в конфиги sing-box 1.14.4.

Назначение
----------
Спайк этапа 1 (S1-T3): доказать, что VLESS + Reality + TCP из подписки провайдера
переносится в outbound sing-box и что итоговый конфиг проходит `sing-box check`.
Продакшн-генератор на Rust появится на этапе 2 и должен повторить маппинг,
зафиксированный здесь.

Вход
----
JSON-массив профилей. У каждого профиля берётся outbound с `tag == "proxy"`.
Остальные ключи профиля (`remarks`, `dns`, `inbounds`, `routing`, `outbounds`,
`burstObservatory`) читаются, но в конфиг переносится только `remarks` (в имя
файла): DNS-политику и route спайк задаёт сам, чтобы проверить интересующие его
правила.

Маппинг Xray -> sing-box (VLESS / Reality / TCP)
------------------------------------------------
| Xray                                              | sing-box                                |
|---------------------------------------------------|-----------------------------------------|
| outbounds[i].tag == "proxy"                       | outbounds[0].tag = "proxy"              |
| outbounds[i].protocol = "vless"                   | outbounds[0].type = "vless"             |
| settings.vnext[0].address                         | server                                  |
| settings.vnext[0].port                            | server_port                             |
| settings.vnext[0].users[0].id                     | uuid                                    |
| settings.vnext[0].users[0].flow                   | flow (только если непустой)             |
| settings.vnext[0].users[0].encryption = "none"    | не переносится (нет аналога)            |
| streamSettings.network = "tcp"                    | транспорт не указывается (TCP по умолч.)|
| streamSettings.security = "reality"               | tls.enabled = true                      |
| realitySettings.serverName                        | tls.server_name                         |
| realitySettings.fingerprint                       | tls.utls.enabled + tls.utls.fingerprint |
| realitySettings.publicKey                         | tls.reality.public_key                  |
| realitySettings.shortId                           | tls.reality.short_id                    |
| realitySettings.spiderX                           | не переносится (нет поля)               |
| tlsSettings.alpn / tlsSettings.fingerprint        | tls.alpn / tls.utls.* (security="tls")  |
| outbounds[i].protocol = "freedom" (tag "direct")  | outbound {"type": "direct"}             |
| outbounds[i].protocol = "blackhole" (tag "block") | не переносится                          |
| outbounds[i].tag = "fragment-out"                 | не переносится (см. ограничения)        |
| dns.servers, dns.queryStrategy                    | не переносится (свой DNS-блок)          |
| routing.rules (geosite/geoip)                     | не переносится (свои route-правила)     |
| burstObservatory                                  | не переносится (URL-проверки задержки)  |

Ограничения спайка (осознанные)
-------------------------------
- Поддерживается только `network = "tcp"`. Профили с ws/grpc/http/h2 пропускаются
  с предупреждением: MVP обязан работать с VLESS + Reality + TCP (PRD §6.1).
- `spiderX` и `encryption` не имеют аналога в sing-box и теряются.
- `fragment-out` (fragment в freedom + sockopt) — трюк Xray для обхода DPI;
  в sing-box 1.14 аналога такому outbound нет, поэтому профиль идёт обычным
  `proxy`. Проверка `tls_fragment` в route — задача следующих этапов.
- `block`/`blackhole` не переносится: в спайке всё лишнее идёт в `direct`.
- Тег `direct` из подписки не переиспользуется — sing-box создаёт свой `direct`.

Использование
-------------
    python3 spike/convert.py --input spike/fixtures/subscription.json \
        --out-dir spike/configs
    python3 spike/convert.py --input ... --out-dir ... --check   # + sing-box check

Коды возврата: 0 — успех, 1 — ошибка входа/записи, 2 — провал `sing-box check`.
Только стандартная библиотека Python 3, без сторонних зависимостей.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

# Транслитерация для стабильных ASCII-имён файлов.
_TRANSLIT = {
    "а": "a", "б": "b", "в": "v", "г": "g", "д": "d", "е": "e", "ё": "e",
    "ж": "zh", "з": "z", "и": "i", "й": "i", "к": "k", "л": "l", "м": "m",
    "н": "n", "о": "o", "п": "p", "р": "r", "с": "s", "т": "t", "у": "u",
    "ф": "f", "х": "kh", "ц": "ts", "ч": "ch", "ш": "sh", "щ": "shch",
    "ъ": "", "ы": "y", "ь": "", "э": "e", "ю": "yu", "я": "ya",
}

# UUID в каноническом виде 8-4-4-4-12.
_UUID_RE = re.compile(
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-"
    r"[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)

# Значения security в streamSettings Xray, которые спайк умеет переносить.
_SUPPORTED_SECURITY = ("reality", "tls")


class ConversionError(Exception):
    """Фатальная ошибка входа: конвертация невозможна целиком."""


@dataclass
class Options:
    """Параметры генерации конфига; значения по умолчанию — сценарий спайка."""

    # DoT-сервер, доступный только через proxy (адрес из TEST-NET-3).
    remote_dns: str = "203.0.113.53"
    # Allowlist: имена процессов и regex по пути процесса.
    process_names: tuple[str, ...] = ("curl",)
    process_path_regex: tuple[str, ...] = (r"^/Applications/Google Chrome\.app/",)
    # mixed-inbound для ручных проверок без TUN.
    mixed_port: int = 10808
    tun_address: str = "172.19.0.1/30"
    tun_mtu: int = 1500


def warn(message: str) -> None:
    """Предупреждение по профилю: не прерывает прогон."""
    print(f"предупреждение: {message}", file=sys.stderr)


def slugify(remarks: str) -> str:
    """Возвращает ASCII-slug из remarks: транслит, нижний регистр, дефисы."""
    out: list[str] = []
    for char in remarks.lower():
        if char in _TRANSLIT:
            out.append(_TRANSLIT[char])
        elif char.isascii() and char.isalnum():
            out.append(char)
        else:
            # Эмодзи, скобки, пробелы — граница слова: имя файла остаётся стабильным.
            out.append("-")
    slug = re.sub(r"-+", "-", "".join(out)).strip("-")
    return slug or "profile"


def load_profiles(raw: bytes, source: Path) -> list[Any]:
    """Разбирает ответ подписки. Ожидается непустой JSON-массив профилей."""
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as error:
        raise ConversionError(
            f"{source}: невалидный JSON ({error.msg}, строка {error.lineno})"
        ) from error
    if not isinstance(payload, list):
        raise ConversionError(
            f"{source}: ожидался JSON-массив профилей, "
            f"получено {type(payload).__name__}"
        )
    if not payload:
        raise ConversionError(f"{source}: массив профилей пуст")
    return payload


def find_proxy_outbound(profile: dict[str, Any]) -> dict[str, Any] | None:
    """Ищет outbound с тегом proxy — единственный источник параметров подключения."""
    outbounds = profile.get("outbounds")
    if not isinstance(outbounds, list):
        return None
    for outbound in outbounds:
        if isinstance(outbound, dict) and outbound.get("tag") == "proxy":
            return outbound
    return None


def build_tls(stream: dict[str, Any], server_name: str) -> dict[str, Any]:
    """Собирает tls-блок sing-box из streamSettings Xray (reality или обычный tls)."""
    security = stream.get("security", "none")
    if security not in _SUPPORTED_SECURITY:
        raise ValueError(f"неподдерживаемая security: {security}")

    tls: dict[str, Any] = {"enabled": True, "server_name": server_name}

    if security == "reality":
        reality = stream.get("realitySettings") or {}
        public_key = reality.get("publicKey")
        if not public_key:
            raise ValueError("в realitySettings нет publicKey")
        tls["utls"] = {
            "enabled": True,
            "fingerprint": reality.get("fingerprint") or "chrome",
        }
        tls["reality"] = {
            "enabled": True,
            "public_key": public_key,
            "short_id": reality.get("shortId", ""),
        }
    else:
        tls_settings = stream.get("tlsSettings") or {}
        if tls_settings.get("alpn"):
            tls["alpn"] = list(tls_settings["alpn"])
        fingerprint = tls_settings.get("fingerprint")
        if fingerprint:
            tls["utls"] = {"enabled": True, "fingerprint": fingerprint}
        if tls_settings.get("allowInsecure"):
            tls["insecure"] = True

    return tls


def convert_profile(
    profile: dict[str, Any], index: int, options: Options
) -> tuple[str, dict[str, Any]] | None:
    """Конвертирует один профиль. None — профиль пропущен с предупреждением."""
    remarks = profile.get("remarks") or f"Профиль {index}"
    label = f"профиль {index} ({remarks})"

    proxy = find_proxy_outbound(profile)
    if proxy is None:
        warn(f"{label}: нет outbound с тегом proxy, пропускаю")
        return None

    protocol = proxy.get("protocol")
    if protocol != "vless":
        warn(f"{label}: protocol={protocol} не поддерживается в этой версии, пропускаю")
        return None

    vnext = (proxy.get("settings") or {}).get("vnext") or []
    if not isinstance(vnext, list) or not vnext:
        warn(f"{label}: пустой settings.vnext, пропускаю")
        return None
    if len(vnext) > 1:
        warn(f"{label}: в vnext {len(vnext)} адреса, использую первый")
    endpoint = vnext[0]
    if not isinstance(endpoint, dict):
        warn(f"{label}: некорректный элемент vnext, пропускаю")
        return None

    address = endpoint.get("address")
    port = endpoint.get("port")
    if not address or not isinstance(port, int) or not 0 < port < 65536:
        warn(f"{label}: некорректные address/port в vnext, пропускаю")
        return None

    users = endpoint.get("users") or []
    if not isinstance(users, list) or not users or not isinstance(users[0], dict):
        warn(f"{label}: в vnext нет users, пропускаю")
        return None
    user = users[0]
    uuid = user.get("id", "")
    if not _UUID_RE.match(uuid):
        warn(f"{label}: некорректный uuid, пропускаю")
        return None

    stream = proxy.get("streamSettings") or {}
    network = stream.get("network", "tcp")
    if network != "tcp":
        warn(f"{label}: network={network} не поддерживается в этой версии, пропускаю")
        return None

    reality = stream.get("realitySettings") or {}
    server_name = reality.get("serverName") or address

    outbound: dict[str, Any] = {
        "type": "vless",
        "tag": "proxy",
        "server": address,
        "server_port": port,
        "uuid": uuid,
    }
    flow = user.get("flow")
    if flow:
        outbound["flow"] = flow

    try:
        outbound["tls"] = build_tls(stream, server_name)
    except ValueError as error:
        warn(f"{label}: {error}, пропускаю")
        return None

    # network = tcp транспорта не требует: TCP — значение по умолчанию в sing-box.
    return remarks, build_config(outbound, options)


def build_config(proxy_outbound: dict[str, Any], options: Options) -> dict[str, Any]:
    """Собирает полный конфиг sing-box: log, dns, inbounds, outbounds, route.

    Схема сверена с sing-box 1.14.4 через `sing-box check`:
    - `route.default_domain_resolver` обязателен с 1.12+, иначе ядро отказывается
      работать на устаревшем поведении;
    - неизвестные поля sing-box отвергает при декодировании (unknown field),
      поэтому в конфиг попадают только поля с аналогом.
    """
    dns_rules: list[dict[str, Any]] = []
    if options.process_names:
        dns_rules.append(
            {"process_name": list(options.process_names), "server": "remote"}
        )
    if options.process_path_regex:
        dns_rules.append(
            {"process_path_regex": list(options.process_path_regex), "server": "remote"}
        )

    return {
        "log": {"level": "info", "timestamp": True},
        "dns": {
            "servers": [
                # DoT через proxy: DNS выбранных приложений не уходит провайдеру.
                {
                    "tag": "remote",
                    "type": "tls",
                    "server": options.remote_dns,
                    "detour": "proxy",
                },
                {"tag": "local", "type": "local"},
            ],
            "rules": dns_rules,
            "final": "local",
        },
        "inbounds": [
            {
                "type": "tun",
                "tag": "tun-in",
                "address": [options.tun_address],
                "mtu": options.tun_mtu,
                "auto_route": True,
                "strict_route": True,
                "stack": "mixed",
            },
            {
                "type": "mixed",
                "tag": "mixed-in",
                "listen": "127.0.0.1",
                "listen_port": options.mixed_port,
            },
        ],
        "outbounds": [proxy_outbound, {"type": "direct", "tag": "direct"}],
        "route": {
            "rules": [
                # Локальный трафик и приватные сети — всегда напрямую и раньше
                # process-правил, чтобы не загнать петлю mixed-inbound в прокси.
                {"domain": ["localhost"], "outbound": "direct"},
                {"ip_is_private": True, "outbound": "direct"},
                *process_rules(options),
            ],
            "final": "direct",
            "auto_detect_interface": True,
            "default_domain_resolver": {"server": "local"},
        },
    }


def process_rules(options: Options) -> list[dict[str, Any]]:
    """Правила allowlist: имя процесса и путь внутри бандла Chrome идут в proxy."""
    rules: list[dict[str, Any]] = []
    if options.process_names:
        rules.append({"process_name": list(options.process_names), "outbound": "proxy"})
    if options.process_path_regex:
        rules.append(
            {"process_path_regex": list(options.process_path_regex), "outbound": "proxy"}
        )
    return rules


def find_sing_box(explicit: str | None) -> Path | None:
    """Ищет бинарь sing-box: явный путь, pinned-бинарь репозитория, затем PATH."""
    if explicit:
        candidate = Path(explicit)
        return candidate if candidate.is_file() else None

    binaries = Path(__file__).resolve().parent.parent / "src-tauri" / "binaries"
    if binaries.is_dir():
        # Имя закреплённого бинаря содержит target triple — берём первый найденный.
        for candidate in sorted(binaries.glob("sing-box-*")):
            if candidate.is_file():
                return candidate

    found = shutil.which("sing-box")
    return Path(found) if found else None


def run_checks(sing_box: Path, files: list[Path]) -> int:
    """Прогоняет `sing-box check` по каждому файлу. Возвращает число провалов."""
    failures = 0
    for path in files:
        result = subprocess.run(
            [str(sing_box), "check", "-c", str(path)],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            print(f"check ok: {path}")
            continue
        failures += 1
        print(f"check FAILED: {path}", file=sys.stderr)
        for stream in (result.stdout, result.stderr):
            if stream.strip():
                print(stream.strip(), file=sys.stderr)
    return failures


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Конвертация подписки Happ (Xray) в конфиги sing-box 1.14.4",
    )
    parser.add_argument("--input", required=True, help="путь к JSON-массиву профилей")
    parser.add_argument("--out-dir", required=True, help="каталог для конфигов")
    parser.add_argument(
        "--check",
        action="store_true",
        help="дополнительно прогнать sing-box check по каждому файлу",
    )
    parser.add_argument(
        "--sing-box",
        default=None,
        help="путь к бинарю sing-box (по умолчанию — pinned-бинарь репозитория)",
    )
    parser.add_argument(
        "--process-name",
        action="append",
        dest="process_names",
        metavar="NAME",
        help="имя процесса для allowlist (можно повторять); по умолчанию curl",
    )
    parser.add_argument(
        "--process-path-regex",
        action="append",
        dest="process_path_regex",
        metavar="REGEX",
        help="regex по пути процесса для allowlist (можно повторять)",
    )
    parser.add_argument(
        "--remote-dns",
        default=None,
        metavar="IP",
        help="адрес DoT-сервера для dns-сервера remote",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    source = Path(args.input)
    out_dir = Path(args.out_dir)

    options = Options()
    if args.process_names is not None or args.process_path_regex is not None:
        # Allowlist задан явно — значения по умолчанию не подмешиваем.
        options.process_names = tuple(args.process_names or ())
        options.process_path_regex = tuple(args.process_path_regex or ())
    if args.remote_dns:
        options.remote_dns = args.remote_dns

    try:
        raw = source.read_bytes()
    except OSError as error:
        print(f"ошибка: не удалось прочитать {source}: {error.strerror}", file=sys.stderr)
        return 1

    try:
        profiles = load_profiles(raw, source)
    except ConversionError as error:
        print(f"ошибка: {error}", file=sys.stderr)
        return 1

    try:
        out_dir.mkdir(parents=True, exist_ok=True)
    except OSError as error:
        print(f"ошибка: не удалось создать {out_dir}: {error.strerror}", file=sys.stderr)
        return 1

    created: list[Path] = []
    skipped = 0
    for number, profile in enumerate(profiles, start=1):
        if not isinstance(profile, dict):
            warn(f"элемент {number}: ожидался объект, пропускаю")
            skipped += 1
            continue
        converted = convert_profile(profile, number, options)
        if converted is None:
            skipped += 1
            continue
        remarks, config = converted
        target = out_dir / f"profile-{number}-{slugify(remarks)}.json"
        try:
            target.write_text(
                json.dumps(config, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
        except OSError as error:
            print(
                f"ошибка: не удалось записать {target}: {error.strerror}", file=sys.stderr
            )
            return 1
        created.append(target)
        print(f"создан: {target}")

    if not created:
        print(
            "ошибка: ни один профиль не сконвертирован "
            f"(пропущено {skipped} из {len(profiles)})",
            file=sys.stderr,
        )
        return 1

    print(f"итого: создано {len(created)}, пропущено {skipped}")

    if args.check:
        sing_box = find_sing_box(args.sing_box)
        if sing_box is None:
            print("ошибка: бинарь sing-box не найден, укажите --sing-box", file=sys.stderr)
            return 2
        print(f"проверка через {sing_box}")
        if run_checks(sing_box, created) > 0:
            return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
