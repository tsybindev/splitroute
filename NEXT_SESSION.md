# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 1 завершён. Этап 2 в работе: подцель **G2.1** (`S2-T1`, `S2-T2`) закрыта.
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Последние коммиты: код G2.1 (`feat:`/`test:`), документация G2.1 (`docs:`).

### Сделано в G2.1

- `package.json` (pnpm, `packageManager: pnpm@11.24.0`), `vite.config.ts`, `tsconfig.json`,
  `eslint.config.js`, `index.html`; React 19 + TS + Tailwind 4 + Zustand + lucide.
- `src/`: оболочка приложения с 5 вкладками (Главная, Локации, Приложения,
  Настройки, Логи), экраны-заглушки, `Card`, тест `App.test.tsx`.
- `Cargo.toml` — workspace: `src-tauri`, `crates/protocol`, `crates/helper`.
- `crates/protocol`: `Request`/`Response`/`Command`/`ErrorCode`, `HelperStatus`,
  `ProtocolError`, модели `Profile`, `AppRule`, `Settings`, `DnsMode`, `LogLevel` и др.
  13 тестов.
- `src-tauri`: Tauri 2 каркас, `tauri.conf.json`, capability, иконка `icons/icon.png`;
  `pnpm tauri dev` запускается.
- `crates/helper`: скелет бинаря (реализация — этап 3).

### Проверено

- `pnpm typecheck`, `pnpm lint`, `pnpm test` (2 теста), `pnpm build` — ok.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` — ok (13 тестов).
- `pnpm tauri dev`: Vite на 1420 + окно приложения; процесс остановлен, порт свободен.

### Не проверено

- CachyOS/Windows: нет машин.
- TUN: нет root (sudo требует пароль) — dev-проверки только через mixed-inbound.
- Реальная подписка: отсутствует; использовать `spike/fixtures/subscription.json`.

### Важные факты окружения

- Rust установлен rustup'ом: `~/.cargo/bin` (rustc 1.99.1, rustfmt/clippy добавлены).
  В каждом shell экспортировать: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Глобально в окружении `NODE_ENV=production` → React грузится production-сборкой
  без `React.act`. В `vite.config.ts` тестам задано `env: { NODE_ENV: "test" }`,
  не убирать.
- TypeScript запинен на **6.0.3**: typescript-eslint 8.x не поддерживает TS 7.
- `pnpm` в домашнем дереве ломался из-за `~/package.json` (`packageManager: yarn`);
  теперь в `app/package.json` свой `packageManager: pnpm@11.24.0`.
- `override_address` у direct-outbound удалён в sing-box 1.13+; в route-правилах
  `action: "route"` с `override_address`/`override_port` работает (проверено в спайке).

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm typecheck && pnpm lint && pnpm test
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
./scripts/fetch-core.sh --verify
```

---

## Промпт следующей подцели: G2.2 (S2-T3, S2-T4)

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G2.2 этапа 2 — S2-T3 (парсер Happ-подписки) и S2-T4
(генератор sing-box конфига). После подцели остановись: коммит кода, отдельный
коммит документации, обновление NEXT_SESSION.md. Следующие подцели не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §2, §3, §10 (архитектура), §12 (git), §13 (DoD), §14, §18.
- PRD.md §6.1 (подписка и Profile), §6.2 (TUN/маршрутизация), §6.3 (allowlist),
  §6.4 (DNS), §9 (поток данных и пример конфига).
- TASKS.md: S2-T3, S2-T4.
- crates/protocol/src/lib.rs и model.rs — контракты уже есть, дублировать нельзя.
- spike/convert.py + spike/fixtures/subscription.json + spike/test_convert.py —
  эталонный маппинг Xray → sing-box, уже проверенный `sing-box check`.
- docs/spikes/stage-1.md — что подтверждено (process-правила TCP/UDP, DNS по процессу).

СРЕДА И ОГРАНИЧЕНИЯ
- TUN без root недоступен; тесты генератора проверяют конфиг через
  `sing-box check`, а не через реальный TUN.
- Реальная подписка отсутствует: фикстура и синтетические значения.
- Секреты (URL подписки, HWID, UUID, реальные адреса) в git не попадают.
- NODE_ENV=production в окружении; для React-тестов уже задано env в vite.config.ts.
- TypeScript 6.0.3, не обновлять до 7 (typescript-eslint).

ЗАДАЧИ

S2-T3, парсер подписки.
- Модуль в `src-tauri/src/subscription/` (чистая функция:
  `parse_subscription(&str) -> Result<Vec<Profile>, SubscriptionError>`).
- Вход — Happ-совместимый JSON: массив профилей, в каждом outbound с тегом proxy.
- Маппинг VLESS + Reality + TCP в `Profile` (crates/protocol). Профили без proxy,
  с неподдерживаемым протоколом/транспортом — пропускаются с предупреждением,
  обновление не падает. Повторить поведение spike/convert.py, но на Rust и в модель.
- Ошибки: битый JSON, не массив, пустой массив, нет proxy, некорректные поля —
  отдельные типизированные варианты (thiserror).
- Тесты: валидный JSON, пустой массив, нет proxy, битый JSON, пропуск неподдерживаемого,
  стабильность id/имени. Использовать `spike/fixtures/subscription.json` как вход
  (можно завести тестовые фикстуры в src-tauri/tests/fixtures/ без секретов).
- Если нужен HTTP-клиент подписки — только на этапе 2/T5; в этой подцели парсер
  принимает строку, сеть не трогаем.

S2-T4, генератор конфига.
- Модуль в `src-tauri/src/config/`: `build_config(&Settings, &Profile, &[AppRule]) -> Result<serde_json::Value, ConfigError>`
  — чистая функция.
- Блоки: log, dns (remote DoT через proxy + local; правила по процессу при
  DnsMode::ProcessSplit, иначе системный), inbounds (TUN с auto_route/strict_route/MTU/stack mixed;
  mixed-inbound по настройкам), outbounds (vless + direct), route (localhost,
  приватные сети, allowlist по process_name/process_path/path_regex, final direct,
  default_domain_resolver).
- IPv6 on/off из Settings; MTU и порт mixed — из Settings.
- Генерировать JSON, совместимый с sing-box 1.14.4. Учесть: `override_address` у
  direct-outbound удалён в 1.13+.
- Golden-тесты: allowlist, direct-only (пустой allowlist), IPv6 on/off, оба DNS-режима.
  Golden-файлы в `src-tauri/tests/fixtures/config/`.
- Интеграционный тест (можно `#[ignore]` или skip при отсутствии бинаря): записать
  конфиг во временный файл и прогнать `sing-box check`; бинарь ищется в
  `src-tauri/binaries/`, при отсутствии — тест пропускается с сообщением.
- Не переносить `fragment-out`, `block`, `burstObservatory` (как в спайке).

ПРОВЕРКИ
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm typecheck && pnpm lint && pnpm test` (если фронтенд не менялся — всё равно прогнать)
- `python3 -m unittest discover -s spike -p 'test_*.py'` (не сломать спайк)

ПОРЯДОК
1. Зафиксируй контракты и сигнатуры; при необходимости добавь типы в crates/protocol,
   а не в src-tauri.
2. Реализуй парсер и генератор, покрой тестами.
3. Коммит кода: feat:/test:. Затем отдельный коммит docs: с TASKS.md и NEXT_SESSION.md.
4. Обнови NEXT_SESSION.md: состояние, что сделано/не сделано, следующий шаг (G2.3).

ЧЕСТНОСТЬ
- Не выдавай непроверенное за проверенное; недоступное — «не проверено» с причиной.
- Секреты не коммить, перед коммитом проверять diff.
- Не оставлять запущенные процессы и временные конфиги.
```

---

## Заметки

- Следующие подцели: G2.3 (S2-T5 клиент helper'а), G2.4 (S2-T6/T7 UI и трей),
  G2.5 (S2-T8 сквозной сценарий).
- Контракты уже в `crates/protocol` — новые типы добавлять туда, не в `src-tauri`.
