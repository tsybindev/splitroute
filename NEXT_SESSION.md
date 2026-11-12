# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 1 завершён. Этап 2 в работе: закрыты подцели **G2.1** (S2-T1, S2-T2) и **G2.2** (S2-T3, S2-T4).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Последние коммиты: код G2.2 (`feat:`/`test:`), документация G2.2 (`docs:`).

### Сделано в G2.2

- `src-tauri/src/subscription/`: парсер Happ-JSON → `Vec<Profile>` (VLESS + Reality/TLS + TCP),
  стабильный `id`, пропуски с причинами; `parse_subscription_report` отдаёт и пропуски.
- `src-tauri/src/config/`: `build_config(&Settings, &Profile, &[AppRule]) -> Value`:
  TUN (auto_route/strict_route/MTU/stack), DNS remote/local + правила по процессу,
  allowlist по `process_name`/`process_path`/`process_path_regex`, IPv6 on/off
  (при выключенном — блок `::/0` для выбранных), смешанный inbound по настройкам.
- Golden-файлы `src-tauri/tests/fixtures/config/*.json`; интеграционный тест
  прогоняет `sing-box check` по сгенерированным конфигам.
- Тесты: 17 unit (src-tauri) + 4 golden + 1 фикстура + 13 protocol.

### Проверено

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` — ok.
- `pnpm typecheck`, `pnpm lint`, `pnpm test` — ok.
- `sing-box check` на сгенерированных конфигах — ok.

### Не проверено

- CachyOS/Windows: нет машин.
- TUN: нет root; проверяются только конфиги, не реальный туннель.
- Реальная подписка: нет; фикстура синтетическая.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell (Rust 1.99.1, rustfmt/clippy есть).
- В окружении `NODE_ENV=production`; тестам React задан `env: { NODE_ENV: "test" }`.
- TypeScript запинен на 6.0.3 (typescript-eslint не поддерживает 7).
- ESLint игнорирует `target/`, `src-tauri/`, `crates/`, `spike/`.
- Golden-файлы обновляются: `UPDATE_GOLDEN=1 cargo test -p splitroute --test config_golden`.
- `override_address` у direct-outbound удалён в sing-box 1.13+; использовать route-правила.
- Прототип helper'а и протокол для справки: `spike/helper/` (Python, dev-режим).

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
```

---

## Промпт следующей подцели: G2.3 (S2-T5)

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G2.3 этапа 2 — S2-T5: клиент helper'а (mock + реальный транспорт),
установка/удаление. После подцели остановись: коммит кода, отдельный коммит
документации, обновление NEXT_SESSION.md. Следующие подцели не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §10 (архитектура), §11 (протокол helper'а), §12, §13, §14, §18.
- PRD.md §6.8 (права и helper), §8 (безопасность), §9 (поток данных).
- TASKS.md: S2-T5.
- crates/protocol/src/lib.rs — Request/Response/Command/ErrorCode/HelperStatus,
  PROTOCOL_VERSION, MAX_MESSAGE_BYTES.
- spike/helper/README.md и spike/helper/helper.py — эталон протокола и поведения
  (сокет 0600, uid пира, только start/stop/status, sing-box check до запуска).

ЗАДАЧИ

1. Клиент helper'а в `src-tauri/src/privileged/`:
   - транспорт: Unix-сокет, JSON по одной строке, лимит MAX_MESSAGE_BYTES,
     таймаут чтения; путь сокета: dev `$TMPDIR/splitroute-helper.sock`,
     production `/var/run/splitroute/helper.sock`; переопределение через env/параметр.
   - проверка PROTOCOL_VERSION и типизированные ошибки (на базе `ErrorCode`).
   - операции: `status`, `start(config)`, `stop`, `install`, `uninstall`.
     `start`/`stop`/`status` идемпотентны.
2. Абстракция для тестов: трейт `HelperClient` + `UnixHelperClient` + `MockHelperClient`.
   Логика вызывающего кода не должна зависеть от конкретного транспорта.
3. Установка/удаление helper'а:
   - интерфейс есть, но в этой сессии сервисы НЕ устанавливаются (нет root).
   - реализуй `Installer`-трейт с `UnsupportedInstaller` (возвращает типизированную
     ошибку «требуются права/этап 3») и mock для тестов; документируй, что реальная
     установка LaunchDaemon/systemd — этап 3.
4. Tauri commands (тонкие обёртки): helper_status, connect/disconnect (пока через
   клиент), helper_install/helper_uninstall. Команды не содержат бизнес-логики.
5. Тесты:
   - mock: start/stop/status, идемпотентность stop, ошибки.
   - реальный клиент против мини-сервера на Unix-сокете внутри теста: успешный
     status, несовпадение версии, неизвестная команда, слишком большое сообщение,
     таймаут/закрытый сокет.
   - проверка, что клиент не принимает ответ с чужим protocol.
6. Не логировать содержимое конфига (UUID/ключи). Ошибки — без секретов.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test

ПОРЯДОК
1. Контракты (трейты, ошибки) — сначала.
2. Реализация и тесты.
3. Коммит кода: feat:/test:. Отдельный коммит docs: (TASKS.md + NEXT_SESSION.md).
4. NEXT_SESSION.md: состояние и промпт G2.4 (S2-T6/S2-T7 UI и трей).

ЧЕСТНОСТЬ
- Не устанавливать сервисы, не трогать системные каталоги, не запускать с sudo.
- Недоступное — «не проверено» с причиной.
- Секреты не коммитить; перед коммитом проверять diff.
```

---

## Заметки

- Дальше: G2.4 (S2-T6/T7 UI и трей), G2.5 (S2-T8 сквозной сценарий).
- Реальная установка helper'а и подпись — этап 3; в G2.3 только интерфейс и mock.
