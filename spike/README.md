# spike/ — эксперименты этапа 1

Каталог для воспроизводимых проверок из `TASKS.md` (этап 1). Продакшн-кода здесь
нет и не должно быть: только фикстуры, скрипты и конфиги, на которых проверяются
риски архитектуры. Итоговый отчёт этапа — `docs/spikes/stage-1.md`.

## Структура

```
spike/
├── README.md                  # этот файл
├── convert.py                 # Xray-профиль -> конфиг sing-box 1.14.4 (golden-логика для этапа 2)
├── test_convert.py            # тесты конвертера (stdlib unittest, без зависимостей)
├── fixtures/
│   ├── README.md              # откуда формат, что синтетическое, как подставить реальную подписку
│   └── subscription.json      # Happ-совместимый JSON: 2 профиля, 4 outbound'а, burstObservatory
└── configs/                   # сгенерированные конфиги sing-box, проверены `sing-box check`
    ├── profile-1-testovaya-lokatsiya-a.json
    └── profile-2-testovaya-lokatsiya-b-fragment.json

spike/macos/                   # S1-T4…T9: process-правила/DNS/UDP без root
├── README.md                  # методика, результаты, что осталось непроверенным
├── process_rules_test.py      # TCP: path_regex, process_name, Chrome
├── udp_rules_test.py          # UDP + process-правила (S1-T9)
├── dns_rules_test.py          # разделение DNS по процессу (S1-T8)
└── socksprobe.go              # SOCKS5-клиент (TCP и UDP), копируется под разными именами

spike/helper/                  # S1-T10: прототип helper'а
├── README.md                  # протокол, безопасность, dev-режим, что не проверено
├── helper.py                  # Unix-сокет 0600, только start/stop/status
├── test_helper.py             # 6 тестов: права, lifecycle, ошибки протокола
└── deploy/                    # черновики LaunchDaemon plist и systemd unit (НЕ ставились)
```

Успешное выполнение стендов на macOS 15.8.2 подтверждает `process_name` и
`process_path_regex` для TCP и UDP, helper-процессы Chrome по `.app` и разделение
DNS по процессу. TUN, macOS 26/27, Linux и Windows этими стендами **не**
покрываются — см. `spike/macos/README.md`.

## Задачи, к которым относится каталог

| ID | Что покрывает |
|---|---|
| S1-T2 | `fixtures/subscription.json` — санитизированная фикстура подписки, 2 профиля, лишние outbound'ы (`direct`, `block`, `fragment-out`) |
| S1-T3 | `convert.py` + `configs/` — конвертация VLESS/Reality/TCP в outbound sing-box, проверка `sing-box check` |
| S1-T4…T7 | используют `configs/*.json` как готовый вход для TUN и process-правил на macOS/Linux/Windows |
| S1-T8 | `macos/dns_rules_test.py` — разделение DNS по процессу (без TUN) |
| S1-T9 | `macos/udp_rules_test.py` — UDP и process-правила (без TUN) |
| S1-T10 | `helper/` — прототип helper'а: сокет 0600, только start/stop/status, конфиг через `sing-box check` |

## Воспроизвести

Из корня репозитория (`app/`):

```bash
# 1. Конвертировать фикстуру (создаёт spike/configs/*.json и печатает список файлов)
python3 spike/convert.py --input spike/fixtures/subscription.json --out-dir spike/configs

# 2. Проверить каждый конфиг ядром (pinned-бинарь из scripts/fetch-core.sh)
./src-tauri/binaries/sing-box-aarch64-apple-darwin check -c spike/configs/profile-1-testovaya-lokatsiya-a.json
./src-tauri/binaries/sing-box-aarch64-apple-darwin check -c spike/configs/profile-2-testovaya-lokatsiya-b-fragment.json

# либо то же одной командой: конвертация + check (бинарь ищется в src-tauri/binaries/)
python3 spike/convert.py --input spike/fixtures/subscription.json --out-dir spike/configs --check

# 3. Тесты конвертера (парсер, маппинг, golden-файлы, ошибки входа)
python3 -m unittest discover -s spike -p 'test_*.py'
```

`sing-box check` при успехе не печатает ничего и возвращает код 0. Любая ошибка
(в том числе устаревшее поле или неизвестный ключ) — повод править не проверку,
а конвертер.

Бинарь ядра в git не лежит: `./scripts/fetch-core.sh` скачивает pinned-версию
(`scripts/core-version.txt`) и сверяет SHA256.

## Что делает `convert.py`

1. Читает JSON-массив профилей подписки.
2. Берёт из каждого профиля outbound с `tag == "proxy"`. Профили без `proxy`,
   а также с неподдерживаемым протоколом или транспортом (`ws`, `grpc`, …)
   **пропускаются с предупреждением в stderr** — прогон не падает (PRD §6.1).
3. Конвертирует `VLESS + Reality + TCP` в outbound sing-box: `server`,
   `server_port`, `uuid`, `flow`, `tls.server_name`, `tls.utls.*`,
   `tls.reality.{public_key, short_id}`. Полная таблица маппинга — в docstring
   `spike/convert.py`.
4. Собирает полный конфиг: `log`, `dns` (`remote` DoT через `proxy` + `local`),
   `inbounds` (TUN `172.19.0.1/30` + `mixed` на `127.0.0.1:10808`), `outbounds`
   (`proxy` + `direct`), `route` (localhost/приватные сети — `direct`,
   allowlist процессов — `proxy`, `final: direct`).
5. Пишет `profile-<номер>-<slug>.json`; slug — транслит `remarks` без эмодзи,
   поэтому имена стабильны между прогонами.

Флаги: `--check` (прогнать `sing-box check`), `--sing-box PATH`,
`--process-name`, `--process-path-regex`, `--remote-dns`.
Коды возврата: `0` — успех, `1` — ошибка входа или записи, `2` — провал `check`.

## Ограничения

- Поддерживается только `VLESS + Reality + TCP` — обязательный набор MVP.
  Другие протоколы и транспорты помечаются как неподдерживаемые, без падения.
- `spiderX`, `encryption`, `fragment-out` (fragment + sockopt), `block` и
  `burstObservatory` из Xray в sing-box не переносятся: у ядра нет прямых аналогов.
  Подробности и последствия — в docstring конвертера и в отчёте этапа.
- Все значения в фикстуре синтетические (TEST-NET-3, `example.com`/`example.net`).
  Реальная подписка подставляется через `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID`
  только на время проверки, см. `fixtures/README.md`.
