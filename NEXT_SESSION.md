# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS GUI/root).
- **Этап 3:** S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично**
  (Linux-пакеты собраны, установка/`systemctl`/TUN/GUI не проверены; Windows VM нет).
- **Этап 4:** S4-T1…S4-T11 — done. **S4-T5c — blocked** (ручная проверка под
  root и Windows VM). **S4-T7 — todo, ждёт решения PRD §13.2**.
- Дата проверки: **2026-11-14**. Ветка `main`. Последний коммит кода — `918c0c6`
  (feat: список запущенных процессов). Документация — отдельный коммит после него.
- Факты: [docs/research.md](./docs/research.md) §19 (статистика), §20 (kill
  switch), §21 (задержки и автовыбор), §22 (автообновление), §23 (запущенные
  процессы).

### Что работает

- **Подключение.** UI → `connect` → `build_config` → helper `start`; helper
  запускает sing-box sidecar и принимает только `status/start/stop`.
  **Протокол helper'а — версия 2.** `install`/`uninstall` по IPC запрещены.
  Установленный helper v1 несовместим: нужна переустановка из настроек.
- **Список запущенных процессов (S4-T11, коммит `918c0c6`).**
  - `RunningApp {pid, name, path, platform}` — в `crates/protocol`; команды
    `apps_running` (`#[tauri::command(async)]`) и `app_rule_from_running`.
  - Процессы читаются без привилегий и внешних команд: macOS —
    `proc_listpids`/`proc_pidpath`, Linux — `/proc/<pid>/exe`, Windows —
    Toolhelp + `QueryFullProcessImageNameW` (фича
    `Win32_System_Diagnostics_ToolHelp`). Платформенный код —
    `src-tauri/src/catalog/running.rs`.
  - Фильтр системных путей, дедупликация по пути (первый pid), сортировка по
    имени без учёта регистра, лимит 500. Причины пропуска — до 20 строк в
    Debug-логе, системные и скрытые записи — счётчиками. Ошибка чтения даёт
    пустой список, вкладка не ломается.
  - Правило создаётся выключенным: macOS — `path_regex ^<escaped .app>/`, если
    путь внутри бандла, иначе `process_path`; Windows/Linux — `process_path`.
  - UI: загрузка, пустое состояние, ошибка с повтором, поиск по имени,
    «Добавить правило», подсказка о выключенном правиле, PID в строке.
    Иконки в «Запущенных» не запрашиваются — процессов сотни.
- **Задержки локаций и автовыбор (S4-T6, коммит `2405d09`).**
  - Замер — TCP-connect до `server:port` в непривилегированном приложении:
    3 с на одно подключение, лимит 64 профиля, 8 параллельных замеров,
    `#[tauri::command(async)]`. Ошибки — `PING_TIMEOUT`, `PING_UNREACHABLE`,
    `PING_DNS`.
  - `profiles_ping` возвращает `results[]`, `best_profile_id` и `switch_to`;
    решение о переключении (гистерезис, подтверждения, серия неудач) — в Rust.
  - `profile_switch`: `stop` → выбор → `start` с тем же kill switch. При ошибке
    старта возвращает прежнюю локацию.
  - UI: «Проверить задержки», мс/ошибка, сортировка и «Лучшая» в «Локациях»;
    карточка автовыбора на «Главной»; настройки с подсказками.
- **Kill switch (S4-T5a/T5b, opt-in fail-closed).**
  - `Settings.network.kill_switch` (default `false`), протокол 2:
    `StartPayload {config, kill_switch}`, `HelperStatus.kill_switch`,
    `ErrorCode::KILL_SWITCH_FAILED`.
  - Helper ставит правила **до** запуска ядра; при падении ядра правила
    остаются (fail-closed); снимаются при `stop`, выходе приложения или
    выключении настройки. macOS — pf anchor `com.apple/splitroute.killswitch`,
    Linux — nftables `inet splitroute_killswitch`, Windows — WFP-провайдер и
    подслой (`crates/helper/src/killswitch/windows.rs`).
  - **Windows (S4-T5b, `8c67018`)** — WFP, не `netsh`: allow-list выражается
    только собственным подслоем с весами. Фильтры статические (переживают
    падение helper'а), при старте helper'а удаляются.
- **Автообновление (S4-T8, `5172dd5`).**
  - Rust-only `tauri-plugin-updater` 2.14.1; команды
    `update_status`/`update_check`/`update_install`, прогресс — событие
    `update-progress`, перезапуск — `AppHandle::restart`.
  - Каналы `stable`/`beta`, манифест
    `https://updates.splitroute.invalid/<channel>/latest.json` — **заглушка** до
    решения PRD §13.3 (`UPDATE_NOT_CONFIGURED` без сети). Debug override —
    `SPLITROUTE_UPDATE_ENDPOINT` (только HTTPS).
  - Подпись: приватный ключ `~/.commandcode/splitroute-updater.key` (+ пароль
    `.key.pass`, права `0600`, в git не попадают), публичный — в
    `src-tauri/tauri.conf.json`; `requireSignedVersion: true`.
- **Статистика трафика (S4-T4).** Локальный Clash API на `127.0.0.1`, случайный
  порт/secret на сессию, `GET /connections` с Bearer; ошибка — «—» без
  `ErrorBox`.
- **Каталог и иконки (S4-T1/T2).** macOS 110 приложений / 100 иконок; Linux
  `.desktop`; Windows ярлыки/реестр. Иконка — отдельная `app_icon(app)`.
- **Подписка (S4-T9/T10).** Happ-JSON, base64 и открытый текст `vless://`; UA по
  умолчанию `SplitRoute/0.1.0`, таймаут 45 с, `connect_timeout` 10 с.
- **Локализация (S4-T3).** `ru`/`en` в Rust, переключение без перезапуска.
- **Helper'ы и установщики.** macOS LaunchDaemon, Linux systemd/polkit, Windows
  служба + named pipe ACL. Установка LaunchDaemon blocked подписью.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1`.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **357 тестов** ok (1 `#[ignore]`).
- `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc
  --all-targets` — чисто.
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **92 теста** ok;
  `pnpm build` ok.
- `pnpm tauri dev` с `SPLITROUTE_MOCK_HELPER=1`: окно `SplitRoute` открывается,
  процесс `target/debug/splitroute` работает; после проверки остановлен, порт
  1420 свободен, `sing-box`/`splitroute` не остались.
- **S4-T11:** реальный скан macOS — 64–68 процессов после фильтра, 345–347
  системных записей, 1 недоступная, 2.1–3.2 мс на весь обход; пути непустые,
  правило из `.app` даёт выключенный `path_regex`. Linux-ветка проверена
  `rustc --target x86_64-unknown-linux-gnu`, Windows-ветка — изолированным
  scratch-крейтом с `windows-sys 0.61` (`cargo check` и `clippy -D warnings`).
- **S4-T6 (тесты):** TCP-probe на loopback, стабильность кодов `PING_*`,
  минимум и порядок при равенстве, лимит 64 профиля, гистерезис и
  подтверждения, серия неудач, `stop → start`, восстановление прежней локации,
  сохранение kill switch, отсутствие адресов серверов в логах, миграция
  настроек, UI-кнопка/сортировка/«Лучшая»/ошибка, автопереключение.
- **sing-box check** и golden-тесты конфигов зелёные: конфиг ядра не менялся.
- **Kill switch (тесты):** тексты правил pf/nft, 6 тестов статического плана
  WFP, идемпотентность enable/disable, ошибка enable → `KILL_SWITCH_FAILED`,
  сохранение правил после «падения», serde протокола 2, mock helper, команда
  `connect` передаёт флаг, UI.
- **Автообновление (тесты):** placeholder → `UPDATE_NOT_CONFIGURED`, URL
  канала/HTTPS, отказ от HTTP, классификация ошибок, стабильные коды,
  отсутствие `download_url`/подписи в ответе, нормализация канала и legacy-
  настройки; 7 сценариев `UpdateCard`.
- **Подписанная сборка (S4-T8):** `./scripts/package.sh` с
  `TAURI_SIGNING_PRIVATE_KEY`/`..._PASSWORD` собрал `SplitRoute.app.tar.gz`
  (34.5 МиБ) и `.sig`; trusted comment содержит `version:0.1.0`.

### Не проверено (причина)

- **S4-T11 на Linux/Windows в рантайме** — ветки компилируются изолированно, но
  реальных ОС нет. Не проверены права на чужие процессы, kernel threads без
  `/proc/<pid>/exe`, пути >260 символов, Store/AppX и Flatpak/Snap.
- **Реальный список в окне** — вкладка проверена тестами и реальным сканом
  macOS; клик по UI в окне не автоматизировался.
- **Реальная задержка серверов провайдера** — нет профиля/сети провайдера.
- **DNS-резолв в замере** не имеет отдельного таймаута.
- **Окно между `stop` и `start`** при переключении: правила kill switch на это
  время снимаются; при ошибке старта локация возвращается.
- **Реальное применение pf/nft/WFP под root** — `sudo -n` не проходит, нет
  Windows VM: **S4-T5c**.
- **Реальная установка обновления** — нет публичного HTTPS-манифеста (домен не
  выбран, PRD §13.3); проверены подпись и UI.
- **Windows/Linux updater-артефакты** — нет VM/машины.
- **Обновление helper'а вместе с приложением** — не реализовано осознанно.
- **Откат неудачной установки** — не реализован.
- **S4-T7 (доменные правила)** — ждёт решения PRD §13.2.
- **Установка/запуск LaunchDaemon** — blocked: нужен Apple Development/Developer ID.
- **TUN, per-app, DNS, сон/пробуждение на упакованных бинарях** — blocked: нет
  root и реальной подписки.
- **Linux-рантайм** (S2-T9, S3-T3, S3-T7) — нет GUI/пароля sudo.
- **Windows-рантайм** (S3-T4, S4-T2, S4-T5b) — нет VM; полный
  `cargo check -p splitroute --target x86_64-pc-windows-msvc` падает на `ring`.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`, Linux —
  `gio-sys`/`glib-sys`. Платформенные ветки проверяйте изолированными крейтами
  или `rustc --emit=metadata --target ...`.
- `sudo -n true` **не проходит**; привилегированные действия — только с явным
  подтверждением пользователя.
- **Debug-сборка** ищет сокет в `$TMPDIR/splitroute-helper.sock`. Установленный
  helper: macOS — `/var/run/splitroute/helper.sock`, Linux —
  `/run/splitroute/helper.sock`, Windows — `\\.\pipe\splitroute-helper`;
  переопределение — `SPLITROUTE_HELPER_SOCKET`/`SPLITROUTE_HELPER_PIPE`.
- **Helper: одно сообщение на соединение.**
- **Установленный helper v1 несовместим с протоколом 2** — переустановите.
- **Bundle id** `com.splitroute.app` заканчивается на `.app` — финальный
  идентификатор решить до публичной сборки (PRD §13.3).
- **Провайдер подписки:** формат зависит от UA; `SplitRoute/0.1.0` → base64
  `vless://` (~1 с), `Happ/4.1.0` → JSON (иногда ~17 с). Таймаут 45 с не уменьшать.
- **CachyOS доступна по SSH** (хост/каталог — в чате, не коммитить). Нет
  git-репозитория: синхронизация `rsync`. `sudo -n` требует пароль, GUI нет.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`). Секреты не логировать.
- **Clash API (S4-T4):** только `127.0.0.1`, случайный порт/secret на сессию;
  endpoint в `state.json` не сохраняется.
- **Kill switch runtime-файлы:** `<runtime>/killswitch.pf` (macOS),
  `killswitch.nft` (Linux), `killswitch.token` (macOS) — права `0600`.
- **Updater-ключ (S4-T8):** `~/.commandcode/splitroute-updater.key` и
  `.key.pass` (права `0600`), публичный — в `tauri.conf.json`. В git не
  коммитить. `SPLITROUTE_ALLOW_UNSIGNED=1` собирает без updater-артефактов.
- **Обновления:** endpoint `.invalid` — заглушка; debug override
  `SPLITROUTE_UPDATE_ENDPOINT`. Подписанные артефакты —
  `target/release/bundle/macos/SplitRoute.app.tar.gz` и `.sig`.
- **Каталог процессов:** `src-tauri/src/catalog/running.rs`; macOS использует
  локальную константу `PROC_ALL_PIDS = 1` (в `libc` её нет). Фильтр системных
  путей — префиксы; `/usr/bin` и `/System/Applications` не отбрасываются.
- **UI-автоматизация:** System Events отвечает; окно `SplitRoute` находится.
- **Скриншоты** с реальным URL/HWID/адресом сервера в репозиторий не коммитить.
- После проверок: `pgrep -x sing-box`, `pgrep -fl 'target/debug/splitroute'`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`,
  `launchctl print system/com.splitroute.helper`.

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
pgrep -x sing-box || echo "ядра нет"
```

---

## Ручные проверки, которые остаются blocked

Выполнять только при появлении root/сертификата/целевых ОС и с явным
подтверждением пользователя. Привилегированные действия автоматически не
запускать.

### Запущенные процессы на Linux/Windows (S4-T11)

Linux (есть машина по SSH):

1. `pnpm tauri dev` и открыть «Приложения → Запущенные».
2. Ожидается непустой список без systemd/DBus-демонов; у выбранного процесса
   «Добавить правило» создаёт выключенное правило с `process_path`.
3. Проверить процесс с удалённым бинарём и kernel thread: вкладка не ломается,
   причина — в Debug-логе.

Windows (VM):

1. Открыть «Запущенные»: нет `C:\Windows\...`, есть пользовательские `.exe`.
2. Правило из процесса — `process_path`, выключено.
3. Проверить процесс с длинным путём (>260) и Store-приложение: ожидаемо либо
   запись с путём, либо пропуск с причиной, но без падения вкладки.

### macOS: kill switch (S4-T5c)

1. `pnpm tauri dev` с реальным helper'ом (не MockHelper), dev-helper под `sudo`
   — см. AGENTS §4.5; включить kill switch в настройках.
2. Подключиться, выбрать приложение с правилом, запустить в нём трафик.
3. `sudo pkill -x sing-box`. Ожидается: прямой трафик выбранного приложения
   блокируется, UI показывает «Ошибка» и кнопку «Отключить и снять защиту».
4. Нажать кнопку (или «Отключить»). Ожидается: `pfctl -a
   com.apple/splitroute.killswitch -s rules` пуст, прямой трафик восстановлен.
5. Проверить, что после перезапуска helper'а «залипших» правил нет.

### macOS: задержки и автопереключение (S4-T6)

1. С реальным профилем нажать «Проверить задержки»: числа сопоставимы с
   `ping`/`nc -z` до тех же серверов; ошибки — локализованные.
2. Включить автовыбор, обновить подписку — выбранная локация самая быстрая.
3. Включить автопереключение, подключиться, дождаться двух проверок: при
   деградации выше порога — `stop → start` без зависшего TUN.
4. При ошибке старта новой локации вернулась прежняя и правила kill switch
   снова применены.

### Обновления (S4-T8)

Выполнять после выбора домена и хостинга манифеста (PRD §13.3).

1. Заменить placeholder в `UPDATE_BASE` (`src-tauri/src/updates.rs`); собрать
   версию N и установить её. Подписать тем же ключом, что в `tauri.conf.json`.
2. Опубликовать `<channel>/latest.json` с версией N+1 и подписью.
3. «Проверить обновления»: версия, канал, заметки; установка только по кнопке.
4. Установить: ядро останавливается, прогресс растёт, приложение
   перезапускается в N+1.
5. Испортить `.sig`: `UPDATE_SIGNATURE`, установка отменена.
6. Без домена: кнопка недоступна, «канал не настроен», сетевых запросов нет.

### Linux (S3-T3, S3-T7, S2-T9, S4-T5c)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
pnpm install
./scripts/fetch-core.sh
./scripts/package.sh             # deb, rpm, appimage
```

Проверить установку/`systemctl`/`pkexec`/TUN/per-app/DNS/kill switch/задержки по
матрице `docs/stage-3-report.md` §9. AppImage без FUSE:
`./SplitRoute_*.AppImage --appimage-extract-and-run`.

### Windows (S3-T4, S4-T2, S4-T5b, S4-T5c)

`./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh`, затем
`./scripts/portable.sh` или `./scripts/package.sh`; проверить службу под SYSTEM,
UAC, pipe ACL, `wintun.dll`, TUN/per-app, NSIS.

Kill switch (WFP) — только на VM/машине, где не страшно потерять сеть:

1. Установить helper (служба под SYSTEM) и включить kill switch.
2. Подключиться; браузер/`curl.exe` ходят через прокси, локальная сеть и DHCP
   работают.
3. `netsh wfp show filters file=%TEMP%\splitroute-filters.xml`: есть провайдер
   «SplitRoute kill switch», блокирующие фильтры на `ALE_AUTH_CONNECT_V4/V6`
   имеют вес ниже разрешений.
4. `Stop-Process -Name sing-box -Force`: прямой `curl.exe` не проходит, UI
   показывает ошибку и кнопку снятия защиты, правила остались.
5. Снять защиту: фильтров SplitRoute нет, прямой трафик восстановлен.
6. Перезапустить helper: «залипших» фильтров нет.
7. `Restart-Service BFE`: статические фильтры теряются до следующего `start` —
   известное ограничение.

---

## Промпт следующей подцели: G4.10 (S4-T7 после решения PRD §13.2 либо backlog)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: следующая незакрытая задача этапа 4.
Сначала открой PRD.md §13 (открытые вопросы, п. 2) и TASKS.md (S4-T7).
Если решение по PRD §13.2 (доменные правила провайдера) уже зафиксировано в
PRD — выполняй S4-T7. Если решения нет — НЕ начинай S4-T7 и НЕ правь PRD
самостоятельно: возьми backlog-задачу B1 «пометка сохранённых правил с
несуществующим путём как „неактивно“ с подсказкой» (TASKS.md, Backlog, PRD §6.3).
Если пользователь в этой сессии уже назвал другую задачу — выполняй её.
После одной подцели остановись: коммит кода/тестов, отдельный коммит
документации, обновление NEXT_SESSION.md. Следующие задачи не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3, §6–§7, §10, §12–§14, §17–§18.
- PRD.md §6.3 (allowlist и вкладки), §7 (экран «Приложения»), §13.2,
  §10 (R2, R3).
- TASKS.md: S4-T7, Backlog (B1), S4-T1, S4-T11.
- docs/research.md §10 (каталог), §23 (запущенные процессы), §21/§22
  (соседние закрытые задачи — не ломай их).
- NEXT_SESSION.md, crates/protocol/src/lib.rs,
  src-tauri/src/catalog/{mod.rs,running.rs}, src-tauri/src/config/,
  src-tauri/src/subscription/, src-tauri/src/store.rs,
  src-tauri/src/commands/mod.rs, src/features/apps/AppsScreen.tsx,
  src/shared/api/{client,types}.ts, src/shared/state/appStore.ts,
  src/shared/i18n/ru.ts и en.ts.

ТЕКУЩЕЕ СОСТОЯНИЕ
- Этап 4: S4-T1…S4-T11 done. Последний коммит кода — `918c0c6`
  (запущенные процессы). S4-T5c blocked (root/Windows VM), S4-T7 ждёт
  решения PRD §13.2.
- Проверено 2026-11-14: 357 Rust-тестов, 92 frontend, fmt/clippy/typecheck/
  lint/build зелёные, `pnpm tauri dev` (MockHelper) открывает окно.
- Если выбран S4-T7: шаблон подписки провайдера содержит `geoip`/`geosite`
  (см. docs/research.md и PRD §13.2); в конфиг они пока не переносятся.
- Если выбрана B1: правила из каталога и «Запущенных» создаются выключенными,
  но сохранённые правила с несуществующим путём ничем не помечены.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- Новых привилегий и shell-команд; helper и его протокол не меняются.
- `crates/protocol` — единственный источник общих моделей; секреты, URL и
  адреса серверов в логи, фикстуры и ответы UI не попадают.
- S4-T7: правила должны оставаться данными (`serde_json::Value`), проходить
  `sing-box check` и golden-тесты; никакого исполнения строк из подписки.
- B1: проверка существования пути не должна блокировать UI и не должна
  выдумывать «неактивно» для regex, который нельзя проверить; при сомнении —
  считать правило активным и не показывать ложную ошибку.
- Не ломай S4-T1/T2/T11: каталог, иконки и список процессов.

ЗАДАЧИ (выбранная ветка)

S4-T7 (только при зафиксированном решении PRD §13.2):
1. Прочитать решение и шаблон подписки; зафиксировать в PRD, какие доменные
   правила переносятся, а какие остаются вне MVP.
2. Rust: перенос `geoip`/`geosite` (или выбранного набора) в route-правила
   генератора конфига; типы — в `crates/protocol`, если они нужны UI.
3. Golden-тесты конфига, `sing-box check`, ручная проверка маршрута.

B1 (если решения по PRD §13.2 нет):
1. Найти, где UI получает `rules_list`, и добавить к правилу вычисляемый
   признак «путь не существует» (Rust): только для абсолютных `process_path`,
   без сети и привилегий; regex — не проверять или проверять консервативно.
2. UI: пометка «неактивно» с подсказкой на «Все правила»; не мешать
   включению/удалению; строки `ru`/`en`.
3. Тесты: существующий/несуществующий путь, regex без ложного срабатывания,
   UI-пометка; старые тесты правил не ломать.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc`
- `pnpm tauri dev` (MockHelper) не должен ломаться; после проверки остановить.

ДОКУМЕНТАЦИЯ
- TASKS.md: статус выбранной задачи и ссылка на коммит; Backlog обновить.
- PRD.md: только если меняется поведение продукта (S4-T7 — по решению §13.2;
  B1 — уточнение §6.3).
- docs/research.md: что проверено, ограничения.
- NEXT_SESSION.md: состояние и самодостаточный промпт следующей подцели.

ОГРАНИЧЕНИЯ
- Не менять helper-протокол, kill switch, обновления, установщики, иконки и
  статистику сверх задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не оставлять dev-серверы и процессы после проверок.

ОТЧЁТ
- TASKS.md: статус со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, промпт следующей подцели.
- Коммиты: feat:/fix:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S4-T11 (запущенные процессы):** macOS — `proc_listpids`/`proc_pidpath`
  (константа `PROC_ALL_PIDS` объявлена локально, в `libc` её нет); Linux —
  `/proc/<pid>/exe`; Windows — Toolhelp. Внешних `ps`/`tasklist` не добавлять.
  Фильтр системных путей — префиксы, `/usr/bin` и `/System/Applications`
  остаются. Иконки в «Запущенных» не запрашивать: это замедляет список.
  Правило из `.app` — regex по внешнему бандлу (первое `.app/` в пути).
  Linux/Windows в рантайме не проверены — не выдавайте компиляцию за проверку.
- **S4-T6 (задержки/автовыбор):** замер — TCP-connect, не прокси-запрос.
  DNS-резолв отдельным таймаутом не ограничен. Переключение — `stop → start`:
  kill switch на мгновение снимается.
- **S4-T8 (обновления):** endpoint `.invalid` — заглушка; не подменять
  публичным URL без решения PRD §13.3. Приватный ключ не коммитить.
  `requireSignedVersion` включён.
- **S4-T5b (Windows):** правила — WFP-провайдер и подслой, не `netsh`. Адреса
  TUN задублированы в `src-tauri/src/config/mod.rs` — меняйте вместе.
- **Kill switch:** протокол 2; после обновления переустановите helper.
- **S4-T4 (статистика):** источник — локальный Clash API; после перезапуска
  окна — «—» до переподключения.
- **S4-T3 (локализация):** новые строки — в `ru.ts` и `en.ts`; тест запрещает
  кириллицу в `en`.
- **S4-T2 (иконки):** не переносите чтение иконок в `apps_installed` и в
  `apps_running`.
- **S3-T2 blocked подписью:** нужен Apple Development/Developer ID.
- **Лицензии:** при смене sing-box/wintun/updater править
  `THIRD_PARTY_NOTICES`; тест `packaging_config` это проверяет.
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
- **Секреты** (URL подписки, HWID, UUID, реальные адреса, `secret` Clash API,
  содержимое pf/nft-правил с адресами) в задачи, фикстуры, логи и коммиты не
  попадают.
