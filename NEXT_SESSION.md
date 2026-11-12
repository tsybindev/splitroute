# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этап 1 завершён. **Этап 2 завершён**: закрыты G2.1 (S2-T1, T2), G2.2 (S2-T3,
  T4), G2.3 (S2-T5), G2.4 (S2-T6, T7), G2.5 (S2-T8).
- S2-T9 (CachyOS) — **blocked**: машины нет, проверка перенесена в этап 3 (S3-T7).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Отчёт этапа 2: [docs/stage-2-report.md](./docs/stage-2-report.md).
- Последние коммиты этапа 2: `097eeef` (бэкенд), `7fe3a12` (UI), `ade86a5`
  (docs G2.4), затем коммиты G2.5 (код и отдельно docs).

### Что работает

- Подписка: загрузка по HTTP (reqwest + rustls), типизированные ошибки
  `MISSING_URL/NETWORK/UNAUTHORIZED/FORBIDDEN/NOT_FOUND/HTTP_ERROR/INVALID_JSON/
  EMPTY/NO_PROFILES`, кэш последнего успешного ответа.
- Профили: парсер Happ-JSON → `Profile` (VLESS + Reality/TLS + TCP), пропуски
  с причинами; автовыбор первого профиля, сохранение выбранной локации.
- Allowlist: `rules_upsert/delete/list`, правила `process_name`, `process_path`,
  `path_regex`; выключенные правила не попадают в конфиг.
- Подключение: конфиг sing-box собирается в ядре, helper получает только объект
  конфига; connect/disconnect, статус, время сессии.
- UI: Главная, Локации, Приложения, Настройки, Логи; русские тексты, состояния
  «нет подписки / ошибка / пусто», кнопка «Повторить», диагностика без секретов.
- Трей и система: меню, single instance, закрытие в трей, autostart-флаг.
- Dev-режим без root: `SPLITROUTE_MOCK_HELPER=1` (только debug) подменяет helper
  на `MockHelperClient`; в лог пишется, что реальный туннель не поднимается.

### Проверено

- `pnpm typecheck && pnpm lint && pnpm test && pnpm build` — ok (22 теста).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace` — ok (63 lib + 14 protocol + 4 golden + 1 integration).
- Сквозной сценарий на уровне ядра и команд: подписка → 2 профиля → выбор второй
  → перезапуск `Store` → правила `process_name`/`path_regex` → выключение правила
  → connect/disconnect через mock helper → конфиг без выключенного правила.
- Реальный HTTP-слой: 401/403/404/500, валидная фикстура, невалидный JSON,
  пустой массив, «нет профилей» — против локального HTTP-сервера.
- `pnpm tauri dev` с mock helper: приложение стартует и остаётся живым.
- Single instance: второй запуск бинаря завершается за 0.2 с, первый процесс
  остаётся один.
- После проверки: приложение и HTTP-сервер остановлены, порты 1420/8765 свободны,
  зависших `splitroute`/`sing-box` и сокета helper'а нет.

### Не проверено (причина)

- Реальный туннель, внешний IP, TUN, маршрутизация выбранных приложений — нет
  root и реальной подписки.
- Трей визуально (иконка, меню, клики) и закрытие окна в трей — нет доступа к
  UI-автоматизации (System Events/screencapture требуют разрешений).
- Регистрация автозапуска в системе — специально не включалась.
- Установка LaunchDaemon/системных сервисов, подпись helper'а — этап 3.
- CachyOS и Windows — нет машин.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- `NODE_ENV=production` в окружении; Vitest задаёт `env: { NODE_ENV: "test" }`.
- TypeScript 6.x (typescript-eslint не поддерживает 7).
- rustls собран с провайдером `ring`; провайдер регистрируется в `run()`.
- Состояние приложения: `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`), права `0600`.
- Unix-сокет helper'а: debug — `$TMPDIR/splitroute-helper.sock`, prod —
  `/var/run/splitroute/helper.sock`; env `SPLITROUTE_HELPER_SOCKET` переопределяет.
- `started_at` от helper'а принимается как целые и дробные секунды (dev-helper
  на Python отдаёт `time.time()`).
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить; в тестах —
  только `example.invalid`, `127.0.0.1` и синтетическая фикстура.

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
```

---

## Ручная проверка этапа 2 (выполнить до этапа 3)

Ниже — то, что нельзя было подтвердить headless. Если пункт не проходит,
зафиксируйте шаги и текст ошибки; следующая сессия начнётся с исправления.

Подготовка (не меняйте реальную подписку, если она нужна: сделайте копию
`~/Library/Application Support/splitroute/state.json`):

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
# терминал 1: локальная фикстура
cd spike/fixtures && python3 -m http.server 8765 --bind 127.0.0.1
# терминал 2: приложение без root
cd app && SPLITROUTE_MOCK_HELPER=1 pnpm tauri dev
```

Чек-лист:

- [ ] Приложение стартует, окно открывается, вкладки переключаются.
- [ ] Пустое состояние: на «Главной» подсказка «Подписка не добавлена», кнопка
      подключения недоступна; на «Локациях» — «Нет локаций» и переход в настройки.
- [ ] Настройки → URL `http://127.0.0.1:8765/subscription.json`, HWID — 16 hex,
      «Сохранить настройки» → «Настройки сохранены».
- [ ] Локации → «Обновить подписку»: 2 локации, первая отмечена «Выбрано»,
      показаны протокол/TLS/адрес.
- [ ] Выбрать вторую локацию, выйти из приложения и запустить снова — выбор
      сохранился.
- [ ] Остановить HTTP-сервер, нажать «Обновить подписку» — понятная ошибка сети
      и кнопка «Повторить»; запустить сервер и повторить — список вернулся.
- [ ] Приложения: добавить `curl` (имя процесса) и Chrome (`^/Applications/Google
      Chrome\.app/`, regex по пути); выключить правило Chrome — статус «Через
      прокси: Нет», правило осталось в списке.
- [ ] Главная → «Подключить»: статус «Подключено», helper показывает
      `mock sing-box 1.14.4`, время сессии идёт; «Отключить» — статус сброшен,
      время `00:00:00`.
- [ ] Логи: есть записи об обновлении подписки и подключении; «Копировать
      диагностику» не содержит URL, HWID и UUID.
- [ ] Трей: закрытие окна при включённом «Закрывать окно в трей» оставляет
      приложение живым; меню показывает статус, «Подключить/Отключить»,
      «Открыть SplitRoute», «Выйти».
- [ ] Single instance: повторный запуск (из терминала или Finder) показывает
      существующее окно, второй процесс не остаётся.
- [ ] Автозапуск не включался; реальный TUN и внешний IP не проверялись — это
      ограничение среды, а не дефект.

После проверки:

```bash
# выйти из приложения через трей, затем
lsof -nP -iTCP:1420 -sTCP:LISTEN || echo "1420 свободен"
lsof -nP -iTCP:8765 -sTCP:LISTEN || echo "8765 свободен"
pgrep -fl 'splitroute|sing-box run' || echo "процессов нет"
```

---

## Промпт следующей подцели: G3.1 (S3-T1, S3-T2)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G3.1 этапа 3 — S3-T1 (Rust-helper) и S3-T2 (macOS LaunchDaemon и
установка/удаление). После подцели остановись: коммит кода, отдельный коммит
документации, обновление NEXT_SESSION.md. Не начинай S3-T3 и дальше.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §8, §10–§14, §17–§18.
- PRD.md §6.8, §9, §10 (R7), §13.
- TASKS.md: S3-T1, S3-T2, этапы 2–3.
- NEXT_SESSION.md (этот файл), docs/stage-2-report.md, docs/research.md.
- Текущее состояние: crates/helper (заглушка), crates/protocol (типы IPC),
  src-tauri/src/privileged/{client,installer,mock,mod}.rs, spike/helper/.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник типов IPC: Request/Response,
  Command::{Install,Uninstall,Start,Stop,Status}, PROTOCOL_VERSION=1,
  MAX_MESSAGE_BYTES=1 МиБ, HelperStatus.
- Протокол: один JSON на строку; запрос {protocol,id,command,payload};
  ответ {protocol,id,ok,payload|error}. Таймаут 10 с.
- Helper принимает только фиксированные команды, не принимает пути от клиента,
  пишет конфиг сам с правами 0600, перед запуском выполняет `sing-box check`,
  не логирует содержимое конфига (UUID/ключи).
- Unix-сокет: права 0600, владелец — пользователь, проверка uid пира
  (SO_PEERCRED на Linux, getpeereid на macOS). Debug — $TMPDIR, prod —
  /var/run/splitroute/helper.sock.
- UI не получает root и не вызывает shell; установка/удаление — только по
  явному действию пользователя с подтверждением.
- Секреты не коммитить и не логировать.

ЗАДАЧИ
1. S3-T1: реализовать Rust-helper в crates/helper (бинарь splitroute-helper)
   по контракту выше; добавить тесты: сериализация/десериализация, неизвестная
   команда, несовпадение версии, лимит сообщения, права сокета, чужой uid
   (если воспроизводимо без root), невалидный конфиг не запускается.
2. S3-T2: macOS LaunchDaemon: plist com.splitroute.helper, бинарь в
   /Library/PrivilegedHelperTools, сокет /var/run/splitroute, установка/удаление
   через HelperInstaller с подтверждением; ad-hoc подпись для dev.
   Реальную установку сервиса выполнять только по явной команде пользователя,
   не в тестах и не автоматически.
3. Проверить, что src-tauri/src/privileged/client.rs совместим с новым helper'ом
   без изменения формата протокола; dev-переключатель SPLITROUTE_MOCK_HELPER=1
   сохранить.
4. Не менять UI-экраны и не начинать S3-T5 (проводка установки в UI) без
   отдельной подцели.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Ручная проверка на macOS: установка LaunchDaemon, `launchctl print
  system/com.splitroute.helper`, права сокета 0600, start/stop/status;
  после проверки удалить сервис и убедиться, что сокета и процесса нет.
- Не оставлять LaunchDaemon, сокеты, sing-box и временные файлы после проверки.

ОГРАНИЧЕНИЯ
- Нет реальной подписки и root-пароля в сессии: TUN и внешний IP не проверять,
  зафиксировать «не проверено» с причиной.
- Не устанавливать системные сервисы без явного запроса пользователя.
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.

ОТЧЁТ
- Обнови TASKS.md (S3-T1, S3-T2), создай/обнови docs/stage-3-report.md с ручной
  матрицей «проверено / не проверено и почему».
- NEXT_SESSION.md: состояние, следующий промпт (G3.2 — S3-T3, S3-T4).
- Коммиты: feat:/fix:/test: для кода, отдельный docs: для документации.
```

---

## Заметки

- Следующие подцели этапа 3: G3.2 — Linux systemd/polkit и `.deb`/`.rpm`/AppImage
  (S3-T3), G3.3 — Windows-служба и `wintun.dll` (S3-T4), G3.4 — проводка установки
  в UI и notices (S3-T5, S3-T6), G3.5 — smoke и отчёт (S3-T7, S3-T8).
- Открытые пункты, вынесенные в backlog: `connect_on_start`, автообновление
  подписки по интервалу, проверка задержки локаций.
- Реальная установка helper'а, подпись и TUN — только этап 3.
