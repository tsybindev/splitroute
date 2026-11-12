# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 1 завершён. Этап 2: закрыты **G2.1** (S2-T1, T2), **G2.2** (S2-T3, T4), **G2.3** (S2-T5).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Последние коммиты: код G2.3 (`feat:`/`test:`), документация G2.3 (`docs:`).

### Сделано в G2.3

- `src-tauri/src/privileged/`:
  - `HelperClient` (status/start/stop) + `HelperClientError` (NotInstalled, Unavailable,
    Timeout, Protocol, Helper{code,message}, InvalidPayload).
  - `UnixHelperClient`: Unix-сокет, JSON по строке, лимит `MAX_MESSAGE_BYTES`,
    таймаут (по умолчанию 10 с), проверка версии протокола, путь сокета из
    `SPLITROUTE_HELPER_SOCKET` / dev `$TMPDIR` / prod `/var/run/splitroute/`.
  - `MockHelperClient` — идемпотентные start/stop/status.
  - `Installer`-трейт + `UnsupportedInstaller` (установка сервиса — этап 3).
- `src-tauri/src/commands/`: `AppState`, команды `helper_status`, `connect`,
  `disconnect`, `helper_install`, `helper_uninstall`; зарегистрированы в `run()`.
- Тесты: 8 клиента (включая версию/ошибку/таймаут/битый payload), 3 mock, 1 installer.

### Проверено

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` — ok (30 тестов в lib splitroute, 13 protocol, 4+1 integration).
- `pnpm typecheck`, `pnpm lint`, `pnpm test` — ok.

### Не проверено

- Реальная установка helper'а и работа под root — этап 3.
- Реальная подписка/сервер — нет; фикстура синтетическая.
- TUN — нет root; CachyOS/Windows — нет машин.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- `NODE_ENV=production` в окружении; React-тестам задан `env: { NODE_ENV: "test" }`.
- TypeScript 6.0.3 (typescript-eslint не поддерживает 7).
- ESLint игнорирует `target/`, `src-tauri/`, `crates/`, `spike/`.
- Путь Unix-сокета ограничен `SUN_LEN` (~104 байта) — в тестах использовать короткие имена.
- Golden-конфиги: `UPDATE_GOLDEN=1 cargo test -p splitroute --test config_golden`.

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
```

---

## Промпт следующей подцели: G2.4 (S2-T6, S2-T7)

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G2.4 этапа 2 — S2-T6 (экраны) и S2-T7 (трей, автозапуск,
single instance). После подцели остановись: коммит кода, отдельный коммит
документации, обновление NEXT_SESSION.md. Следующую подцель не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §2 (структура src/), §10, §12, §13, §14, §18.
- PRD.md §5 (M1–M10), §6.1 (подписка), §6.3 (allowlist), §6.5 (трей/автозапуск),
  §6.6 (настройки), §6.7 (логи), §7 (UX, состояния и ошибки).
- TASKS.md: S2-T6, S2-T7.
- crates/protocol — Profile, AppRule, Settings, HelperStatus.
- src-tauri/src/{subscription,config,privileged,commands} — что уже есть.
- NEXT_SESSION.md (этот файл).

СРЕДА И ОГРАНИЧЕНИЯ
- TUN без root недоступен; подключение проверять через mock helper или dev-режим
  spike/helper/helper.py (без TUN).
- Реальной подписки нет: использовать spike/fixtures/subscription.json; в UI
  предусмотреть состояния «нет подписки», «ошибка», «пустая подписка».
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- Не устанавливать автозапуск и сервисы в системе при тестах; проверять код и
  конфигурацию, а реальную регистрацию отмечать как «не проверено».

ЗАДАЧИ

S2-T6, экраны (React + TS + Zustand + i18n ru):
- Главная: большая кнопка подключения, статус (helper/подписка/DNS/allowlist),
  текущая локация, время сессии, переход к локациям.
- Локации: список Profile, выбор активного, протокол/адрес, кнопка обновления,
  пустое состояние.
- Приложения: список правил AppRule, переключатель enabled, добавление вручную
  (process_name / путь / regex), пояснение про процессы и дочерние процессы.
- Настройки: URL подписки, HWID (генерация/редактирование), User-Agent, MTU,
  IPv6, mixed-inbound (порт), DNS-режим, уровень логов, статус helper.
- Логи: список строк, фильтр по уровню, копирование диагностики без секретов.
- Все состояния и ошибки из PRD §7; тексты через словарь i18n (ru), без
  захардкоженных строк в компонентах.
- Обёртки Tauri commands/events в src/shared/api; UI не формирует config.

Бэкенд-команды, которых не хватает для UI (добавить тонко, без бизнес-логики в UI):
- получение подписки: `subscription_fetch(url, hwid, user_agent)` — HTTP через
  reqwest с rustls, парсинг `subscription::parse_subscription`, кэш последнего
  успешного ответа; ошибки типизированы (сеть, 401/403/404, невалидный JSON,
  пустой список);
- хранение активных Profile/Settings/AppRule в состоянии приложения (в памяти +
  файл в пользовательском каталоге; без секретов в логах);
- `profiles_list`, `rules_list`, `rules_upsert`, `rules_delete`, `settings_get`,
  `settings_update`.
Секреты — только в пользовательском конфиге, не в git и не в диагностике.

S2-T7, трей и системная интеграция:
- Иконка и меню трея: статус, подключить/отключить, открыть, выход; иконка
  отражает состояние.
- Автозапуск: tauri-plugin-autostart; опции «старт свёрнутым» и «подключаться
  при старте» (только сохранение и передача флага; авто-подключение — позже).
- Single instance: tauri-plugin-single-instance.
- Обнови capabilities/permissions для плагинов.
- Закрытие окна сворачивает в трей (настраивается).

ПРОВЕРКИ
- pnpm typecheck && pnpm lint && pnpm test (Vitest + Testing Library, ключевые
  состояния экранов; api замокать).
- cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
  && cargo test --workspace.
- pnpm build.
- `pnpm tauri dev` — приложение стартует, окно открывается; трей в headless-проверке
  не подтвердить — отметить как «не проверено» с причиной.

ПОРЯДОК
1. Контракты команд и типы (при необходимости — в crates/protocol).
2. Бэкенд-команды + тесты, затем UI + тесты.
3. Коммит кода: feat:/test:/refactor:. Отдельный коммит docs: (TASKS + NEXT_SESSION).
4. NEXT_SESSION.md: состояние и промпт G2.5 (S2-T8 сквозной сценарий).

ЧЕСТНОСТЬ
- Не выдавай непроверенное за проверенное; недоступное — «не проверено» с причиной.
- Не оставлять запущенные процессы и временные файлы; секреты не коммитить.
```

---

## Заметки

- Дальше: G2.5 (S2-T8 сквозной сценарий на macOS; CachyOS — если машина появится).
- Реальная установка helper'а и подпись — этап 3, не раньше.
