# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 1 завершён. Этап 2: закрыты **G2.1** (S2-T1, T2), **G2.2** (S2-T3, T4),
  **G2.3** (S2-T5), **G2.4** (S2-T6, T7).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Последние коммиты G2.4: `097eeef` (бэкенд), `7fe3a12` (UI), затем `docs:`.

### Сделано в G2.4

- Бэкенд:
  - `subscription_fetch` (reqwest + rustls, заголовки `X-HWID`, `Accept`, `User-Agent`);
    типизированные ошибки `MISSING_URL/NETWORK/UNAUTHORIZED/FORBIDDEN/NOT_FOUND/
    HTTP_ERROR/INVALID_JSON/EMPTY/NO_PROFILES`; кэш последнего успешного ответа.
  - `src-tauri/src/store/`: настройки (подписка, сеть, поведение), профили,
    выбранная локация, правила; файл в пользовательском каталоге, права `0600`.
  - `profiles_list`, `profile_select`, `rules_list`, `rules_upsert`, `rules_delete`,
    `settings_get`, `settings_update`, `hwid_generate`, `logs_list`, `diagnostics_get`.
  - `connect`/`disconnect` больше не принимают конфиг из UI: конфиг собирается в ядре.
  - `logging`: кольцевой буфер логов; диагностика без URL/HWID/UUID. Ответы в UI не
    содержат исходный JSON профиля (`raw`).
- Трей и система:
  - меню: статус, подключить/отключить, открыть, выход; статус обновляется из меню и
    из UI через событие `connection-changed`;
  - `tauri-plugin-single-instance` (повторный запуск показывает окно);
  - `tauri-plugin-autostart` (флаг применяется при сохранении настроек);
  - закрытие окна в трей по `close_to_tray`, старт свёрнутым по `launch_minimized`;
  - capabilities: `autostart:default` и window-права.
- UI (`src/`):
  - экраны Главная, Локации, Приложения, Настройки, Логи по PRD §7;
  - все строки в `src/shared/i18n/ru.ts`; состояния «нет подписки / ошибка / пусто»;
  - Zustand-состояние `src/shared/state/appStore.ts`, обёртки в `src/shared/api`;
  - UI не формирует конфиг и не вызывает `invoke` напрямую.

### Проверено

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` — ok (57 тестов lib splitroute, 13 protocol, 4+1 integration).
- `pnpm typecheck`, `pnpm lint`, `pnpm test` (18 тестов), `pnpm build` — ok.
- `pnpm tauri dev` — приложение собирается и стартует, процесс работает без падений;
  после проверки остановлено, порт 1420 свободен.

### Не проверено (причина)

- Визуальный трей (иконка/меню/клики) — headless-проверка невозможна.
- Реальная регистрация автозапуска в системе — не включалась специально.
- Single instance визуально (второй запуск) — не проверялось.
- Подключение через TUN — нет root; проверялся mock helper, без реального туннеля.
- Реальная подписка/сервер — нет; используются `spike/fixtures/subscription.json`.
- CachyOS/Windows, реальная установка helper'а — нет машин, этап 3.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- `NODE_ENV=production` в окружении; React-тестам задан `env: { NODE_ENV: "test" }`.
- TypeScript 6.x (typescript-eslint не поддерживает 7).
- rustls собран с провайдером `ring`; провайдер регистрируется в `run()`.
- Состояние приложения: `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`).
- Путь Unix-сокета helper'а ограничен `SUN_LEN` (~104 байта).

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
```

---

## Промпт следующей подцели: G2.5 (S2-T8)

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G2.5 этапа 2 — S2-T8, сквозной сценарий «подписка → локация →
подключение → allowlist» на macOS. После подцели остановись: коммит кода (если он
менялся), отдельный коммит документации, обновление NEXT_SESSION.md. Этап 3 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §10, §12–§14, §17–§18.
- PRD.md §6.1, §6.3, §6.5, §7, §11, §13.
- TASKS.md: S2-T8, этапы 2–3.
- NEXT_SESSION.md (этот файл).
- Текущее состояние: src-tauri/src/{commands,store,subscription,config,privileged,tray,logging},
  src/shared/{api,state,i18n}, src/features/*.

СРЕДА И ОГРАНИЧЕНИЯ
- Реальной подписки нет: использовать spike/fixtures/subscription.json и временный
  локальный HTTP-сервер (`python3 -m http.server`) или прямой вызов с mock fetcher.
- TUN без root недоступен: подключение проверять через MockHelperClient или
  dev-режим spike/helper/helper.py (без TUN). Реальный туннель и внешний IP —
  «не проверено» с причиной.
- Не устанавливать автозапуск и системные сервисы; реальную регистрацию отмечать
  как «не проверено».
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- NODE_ENV=production в окружении не убирать; TypeScript 6.x.

ЗАДАЧИ
1. Прогнать сквозной сценарий на macOS:
   - пустое состояние: нет подписки → экран Локации и Главная показывают подсказки;
   - ошибки подписки: 401/403/404, невалидный JSON, пустой список — понятные тексты
     и повтор;
   - успешное обновление из фикстуры: 2 профиля, первый выбран автоматически;
   - выбор второй локации, сохранение выбора после перезапуска приложения;
   - добавление правила allowlist (process_name и path_regex), выключение правила;
   - подключение/отключение через mock helper, статус и время сессии на Главной;
   - закрытие окна в трей и повторный запуск (single instance) — если headless не
     подтверждается, зафиксировать «не проверено» с причиной.
2. Исправить найденные дефекты минимальными правками; каждое исправление — с тестом,
   если это логика, а не только проводка.
3. Обновить ручную матрицу в отчёте: что проверено, что нет и почему.
4. Коммит: feat:/fix:/test: для кода и тестов, отдельный docs: для TASKS и NEXT_SESSION.
5. NEXT_SESSION.md: итог этапа 2, готовый промпт этапа 3 (helper'ы и упаковка).

ПРОВЕРКИ
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
- pnpm tauri dev: старт приложения; после проверки остановить, порт 1420 освободить.
- Не оставлять временные HTTP-серверы, helper'ы и TUN-интерфейсы.

ЧЕСТНОСТЬ
- Не выдавай непроверенное за проверенное; недоступное — «не проверено» с причиной.
- Если сценарий упирается в отсутствие root/подписки, оформи это как ограничение
  среды, а не как дефект кода.
```

---

## Заметки

- Дальше: G2.5 (S2-T8). Реальная установка helper'а, подпись и TUN — этап 3.
- После этапа 2 в отчёте должно быть явно: какие пункты матрицы PRD не проверены
  из-за отсутствия root, реальной подписки и машин с CachyOS/Windows.
