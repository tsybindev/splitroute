# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 1 в работе. Закрыты `S1-T1…S1-T3` и подцели G1 (`S1-T4…T7`) и G2 (`S1-T8…T10`).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Последние коммиты: код G2 (`feat:`/`test:`), документация G2 (`docs:`).

### Что подтверждено запуском (macOS 15.8.2 arm64, sing-box 1.14.4)

- `process_name` и `process_path_regex` для TCP — `spike/macos/process_rules_test.py`.
- helper-процессы Chrome матчатся по каталогу `.app` (подтверждает R3 и митигацию).
- process-правила для UDP — `spike/macos/udp_rules_test.py`.
- разделение DNS по процессу на уровне DNS-модуля — `spike/macos/dns_rules_test.py`.
- прототип helper'а: сокет 0600, uid пира, только start/stop/status, `sing-box check`
  до запуска, 6 тестов — `spike/helper/`.

### Что НЕ проверено (причины)

- TUN: нет root (`sudo -n` требует пароль) → `configure tun interface: operation not permitted`.
- Сравнение внешнего IP: нет реальной подписки/сервера (`SUBSCRIPTION_URL` не задан).
- macOS 26/27 (R1): нет машины. CachyOS: нет машины, Docker-демон не запущен.
- Windows: нет VM.
- DNS-утечки через TUN, `systemd-resolved`, Private Relay; QUIC поверх TUN.
- Установка LaunchDaemon/systemd и подпись helper'а — этап 3.

### Факты окружения

- macOS 15.8.2 (24H232), arm64; Rust/cargo нет; Go 1.27.0 есть; Python 3.14.7.
- `override_address` у direct-outbound удалён в sing-box 1.13+; используется
  route-правило с `override_address`/`override_port` или `socks`-outbound.
- Сообщения протокола helper'а — по одному JSON на строку, лимит 1 МиБ.

### Проверка перед стартом

```bash
cd app
git status --short && git log --oneline -3
./scripts/fetch-core.sh --verify
python3 -m unittest discover -s spike -p 'test_*.py'        # 16 тестов
python3 spike/macos/process_rules_test.py --no-chrome
python3 spike/macos/udp_rules_test.py
python3 spike/macos/dns_rules_test.py
pgrep -fl 'sing-box run' || echo "sing-box не запущен"
```

---

## Промпт следующей подцели: G3 (S1-T11…S1-T12)

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app

ЦЕЛЬ: закрыть этап 1 — S1-T11 (итоговый отчёт docs/spikes/stage-1.md) и
S1-T12 (handoff этапа 2). После подцели остановись, этап 2 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- PRD.md §6.2, §6.3, §6.4, §10 (риски R1–R6, R13), §13 (открытые вопросы).
- AGENTS.md §10, §12, §13 (DoD), §14, §17 (этапы), §18 (лимиты).
- TASKS.md: статусы S1-T1…S1-T10 и критерии S1-T11, S1-T12.
- docs/research.md, особенно §7.
- spike/README.md, spike/macos/README.md, spike/helper/README.md, NEXT_SESSION.md.

СОСТОЯНИЕ (не перепроверяй заново, но можешь прогнать команды проверки)
- Подтверждено на macOS 15.8.2 / sing-box 1.14.4: process-правила для TCP и UDP,
  helper-процессы Chrome по .app, разделение DNS по процессу, прототип helper'а.
- Не проверено: TUN (нет root), внешний IP (нет реальной подписки), macOS 26/27,
  CachyOS, Windows VM, DNS-утечки через TUN/systemd-resolved/Private Relay,
  QUIC поверх TUN, установка сервисов helper'а.

ЗАДАЧИ

S1-T11, отчёт docs/spikes/stage-1.md.
- Методика по каждой проверке и что именно измерялось.
- Точные версии ОС и ядра, команды, результаты.
- Ключевые выдержки логов (обезличенные; без UUID, HWID, URL подписки, реальных адресов).
- Go/no-go по каждой платформе: macOS 15, macOS 26/27, Linux, Windows.
  Для непроверенного — «no-go по данным»/«не проверено» с причиной, а не «работает».
- Вывод по macOS и по архитектуре в целом (sing-box sidecar + TUN + process-правила + helper).
- Обнови риски PRD §10 по фактическим результатам, не подгоняя формулировки:
  R1 (macOS 27) — непроверен; R2/R3 — подтверждены и митигируются правилами;
  R4 — UDP подтверждён, QUIC нет; R6 — DNS по процессу подтверждён, утечки нет данных.
  Если меняешь поведение/решения — правь PRD в том же коммите документации.
- Обнови docs/research.md §6/§7 фактическими результатами и датой.

S1-T12, handoff этапа 2.
- TASKS.md: обнови статусы S1-T11, S1-T12 и заголовок этапа 1 (done, если отчёт есть;
  иначе явно перечисли, что осталось).
- NEXT_SESSION.md: перезапиши под этап 2. Промпт должен быть самодостаточным:
  цель этапа 2, задачи S2-T1…S2-T8 из TASKS.md, файлы/модули, контракты
  (crates/protocol), ограничения, команды проверки, формат отчёта.
  Включи известные ограничения из этапа 1 (нет TUN без root, нет Rust-тулчейна до установки,
  нет реальной подписки — использовать фикстуру).
- Не начинай этап 2 в этой же сессии.

ПОРЯДОК
1. Сверь факты с отчётами стендов; при расхождении — разбирайся, не подгоняй.
2. Напиши отчёт, прогони проверки (тесты, `sing-box check`, стенды).
3. Коммит документации: docs: (отчёт + TASKS + NEXT_SESSION + research/PRD при необходимости).
4. Отчёт о результате: что проверено, что нет, go/no-go, следующий этап.

ЧЕСТНОСТЬ
- Недоступная ОС/права — «не проверено» с причиной.
- Никаких секретов в отчётах, логах и коммитах: только обезличенные наблюдения.
- Перед коммитом проверь diff на URL подписки, HWID, UUID, ключи и реальные адреса.
```

---

## Заметки для G3

- В отчёте не дублировать весь `spike/macos/README.md`; дать сводку и ссылки.
- Платформенные результаты честно разделить: «подтверждено запуском» и «не проверено».
