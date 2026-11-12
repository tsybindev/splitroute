# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждого этапа или подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- **Этап 1 завершён** (с оговорками по непроверенным платформам). Этап 2 не начат.
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Отчёт этапа 1: `docs/spikes/stage-1.md`.

### Итог этапа 1

- **Подтверждено запуском** (macOS 15.8.2 arm64, sing-box 1.14.4): process-правила
  для TCP и UDP, матчинг helper-процессов Chrome по каталогу `.app`, разделение DNS
  по процессу, прототип helper'а (сокет 0600, uid пира, start/stop/status).
- **Не проверено:** TUN целиком (нет root), внешний IP (нет реальной подписки),
  macOS 26/27 (R1), CachyOS, Windows, DNS-утечки через TUN/systemd-resolved/Private Relay,
  QUIC поверх TUN, установка сервисов helper'а.
- Go/no-go: условный **go** для macOS 15; остальные платформы — «нет данных».

### Факты окружения (важно для этапа 2)

- macOS 15.8.2 (24H232), arm64.
- Node 22.23.2 и corepack 0.34.6 есть; **Rust/cargo отсутствует** — ставить по
  AGENTS.md §4.2.
- **pnpm в домашнем дереве падает** с «This project is configured to use yarn»:
  в `~/package.json` указан `packageManager: yarn@3.6.4`. Лечение: задать в
  `app/package.json` свой `"packageManager": "pnpm@<версия>"` (или
  `COREPACK_ENABLE_PROJECT_SPEC=0` на время). В `/tmp` pnpm работает.
- sudo без пароля недоступен → TUN при разработке недоступен; для dev-проверок
  использовать mixed-inbound без TUN.
- Реальной подписки нет: парсер и UI проверять на `spike/fixtures/subscription.json`.
  Секреты — только через `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID`, не в git.
- Полезные факты из этапа 1: пути процессов на macOS канонические
  (`/private/var/...`); `override_address` у direct-outbound удалён в sing-box 1.13+
  (использовать route-правила).

### Проверка перед стартом

```bash
cd app
git status --short && git log --oneline -3
./scripts/fetch-core.sh --verify
python3 -m unittest discover -s spike -p 'test_*.py'   # 16 тестов
pgrep -fl 'sing-box run' || echo "sing-box не запущен"
```

---

## Промпт этапа 2: ядро MVP

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TypeScript + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app

ЦЕЛЬ: этап 2 из TASKS.md — ядро MVP: подписка → Profile → генератор конфига,
connect/disconnect, allowlist, трей. Этап 2 большой: после каждой подцели
(см. ниже) делай контрольную точку — коммит кода, отдельный коммит документации,
обновление NEXT_SESSION.md. Не начинай этап 3.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md целиком, особенно §2 (структура), §3 (стек), §4–§7 (среда), §10
  (архитектура), §12 (git), §13 (DoD), §14 (правила агентов), §17 (этапы), §18 (лимиты).
- PRD.md: §5 (M1–M10), §6 (функциональные требования), §7 (UX), §8 (НФТ), §9 (архитектура), §13.
- TASKS.md: этап 2, задачи S2-T1…S2-T8.
- docs/spikes/stage-1.md и docs/research.md §8 — что уже проверено и какие ограничения.
- NEXT_SESSION.md (этот файл).

СРЕДА (проверь, не предполагай)
- Rust/cargo отсутствует: установи по AGENTS.md §4.2 (rustup, stable) и подтверди `rustc --version`.
- pnpm в домашнем дереве падает из-за ~/package.json (packageManager yarn). Прежде чем
  запускать pnpm, задай в app/package.json свой "packageManager": "pnpm@<версия>"
  или используй COREPACK_ENABLE_PROJECT_SPEC=0. Проверь `pnpm --version` из app/.
- sudo без пароля нет → TUN нельзя; dev-проверки connect делать через mixed-inbound
  без TUN. Не выдавай непроверенное за проверенное.
- Реальная подписка не нужна: использовать spike/fixtures/subscription.json.
  Секреты только через SUBSCRIPTION_URL/SUBSCRIPTION_HWID, в git не попадают.

ПОДЦЕЛИ ЭТАПА 2 (строго по порядку, контрольная точка после каждой)

G2.1 — S2-T1, S2-T2: скелет и контракты.
- Tauri 2 + React 19 + TS + Vite + Tailwind 4 + Zustand; структура каталогов по AGENTS.md §2.
- Workspace Cargo: src-tauri, crates/protocol, crates/helper (helper пока скелет).
- crates/protocol: типы IPC (Request/Response, версия протокола, коды ошибок),
  Profile, AppRule, Settings, состояние подключения. Только типы, без дублей.
- Команды: pnpm typecheck, lint, test; cargo fmt --check, clippy -D warnings, test.

G2.2 — S2-T3, S2-T4: подписка и конфиг.
- Парсер Happ-JSON: массив профилей, outbound с тегом proxy, VLESS+Reality+TCP;
  профили без proxy/с неподдерживаемым протоколом пропускаются с предупреждением.
- Тесты: валидный JSON, пустой массив, нет proxy, битый JSON, HTTP-ошибки (мок клиента).
- Генератор sing-box конфига как чистая функция: TUN (auto_route, strict_route,
  MTU, stack mixed), DNS remote/local + правила по процессу, route (localhost,
  приватные сети, allowlist, final direct), IPv6 on/off.
- Golden-тесты. Маппинг сверить с spike/convert.py — он уже прошёл sing-box check.
  Учесть: override_address у direct-outbound удалён в 1.13+.

G2.3 — S2-T5: клиент helper'а.
- Клиент IPC (Unix socket / named pipe), mock-режим для тестов.
- Установка/удаление helper'а: только через предусмотренный интерфейс; в dev —
  mock или запуск helper.py из spike/helper как референс протокола.
- start/stop/status идемпотентны, ошибки типизированы. Протокол — из crates/protocol.

G2.4 — S2-T6, S2-T7: UI и трей.
- Экраны: Главная, Локации, Приложения, Настройки, Логи; все состояния и ошибки
  из PRD §7; русские тексты в словаре i18n (ru).
- Трей: статус, подключить/отключить, открыть, выход; автозапуск; single instance.
- Обёртки Tauri commands/events в src/shared/api. UI не формирует конфиг и не знает
  путей ядра.

G2.5 — S2-T8: сквозной сценарий.
- Подписка (фикстура) → локация → подключение → allowlist → статус.
- Прогнать ручную матрицу PRD на macOS; на CachyOS — только если машина доступна,
  иначе зафиксировать «не проверено».

ВНУТРИ КАЖДОЙ ПОДЦЕЛИ
1. Зафиксируй контракты (типы, сигнатуры, формат) до параллельной работы.
2. Делегируй ≤3 субагентам без пересечения файлов; они не коммитят и не правят
   PRD/AGENTS/TASKS/NEXT_SESSION.
3. Прогони проверки: pnpm typecheck && pnpm lint && pnpm test;
   cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
   && cargo test --workspace.
4. Коммит кода (feat:/fix:/test:/refactor:), затем отдельный коммит docs:
   (TASKS.md + NEXT_SESSION.md, при изменении поведения — PRD).
5. Обнови NEXT_SESSION.md: состояние, что сделано/не сделано, следующий шаг.

ЧЕСТНОСТЬ И ГИГИЕНА
- Недоступная ОС/права/сервер — «не проверено» с причиной.
- Не коммить URL подписки, HWID, UUID, ключи, реальные адреса; перед коммитом
  проверять diff.
- Не оставлять запущенные ядра, helper'ы, dev-серверы и временные конфиги.
- Не выполнять разрушительные команды и не устанавливать сервисы без явной цели.

ОТЧЁТ В КОНЦЕ КАЖДОЙ ПОДЦЕЛИ
- Что сделано и какие файлы изменены.
- Как проверено: команды и результат.
- Что не проверено и почему.
- Статусы S2-задач и следующий шаг.
```

---

## Заметки

- Этап 2 не должен молча менять продуктовые требования: правка поведения — через PRD.
- Итог этапа 1 не переписывать «под удобство»: непроверенные платформы остаются
  непроверенными до отдельных прогонов.
