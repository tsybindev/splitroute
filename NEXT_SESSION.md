# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS GUI/root).
- **Этап 3:** S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично**
  (Linux-пакеты собраны, установка/`systemctl`/TUN/GUI не проверены; Windows VM нет).
- **Этап 4:** S4-T1…S4-T14 — done. **S4-T5c — blocked** (ручная проверка под
  root и Windows VM). **S4-T7 — closed** решением A (PRD §13 п. 7): доменные
  правила провайдера в MVP не переносятся, новый код не потребовался.
- Дата проверки: **2026-11-14**. Ветка `main`. Последний коммит кода — `168a089`
  (feat: автообновление подписки по `auto_update_hours`), документация —
  отдельный коммит после него.
- **CI:** `.github/workflows/build.yml`, репозиторий публичный
  (`github.com/tsybindev/splitroute`). Прогон 2026-11-14 зелёный: `windows-latest`
  → NSIS `.exe`, `ubuntu-22.04` → `.deb`/`.rpm`/`.AppImage`, ~6 мин с кэшем;
  артефакты `splitroute-windows`/`splitroute-linux` в разделе Actions. Пакеты
  неподписанные (`SPLITROUTE_ALLOW_UNSIGNED=1`), macOS-job нет.
- Факты: [docs/research.md](./docs/research.md) §19 (статистика), §20 (kill
  switch), §21 (задержки и автовыбор), §22 (автообновление приложения), §23
  (запущенные процессы), §24 (пометка несуществующих путей), §25
  (автоподключение), §26 (доменные правила — решение A), §27 (CI Windows/Linux),
  §28 (автообновление подписки).

### Что работает

- **Автообновление подписки по интервалу (S4-T14, коммит `168a089`).**
  - `src-tauri/src/autoupdate.rs`; запуск — `setup` в `lib.rs` после трея.
    Планировщик — `tauri::async_runtime::spawn_blocking`, новых зависимостей нет.
  - Расписание: `next_update_at = max(profiles_fetched_at, last_attempt) +
    auto_update_hours × 3600`. `0` — выключено, пустой URL — загрузка не
    запускается, первая попытка при запуске, если данных нет.
  - Ошибка сети сохраняет кэш и выбранную локацию; попытка запоминается и при
    ошибке, поэтому сломанный провайдер не опрашивается на каждом тике
    (проверка расписания раз в минуту).
  - Одна загрузка за раз: `FetchGuard` в `AppState` (флаг `AtomicBool`,
    снимается в `Drop`). Планировщик в этом случае получает `Outcome::Busy`,
    команда — `FETCH_IN_PROGRESS`; UI не показывает это как ошибку.
  - Успех — событие `subscription-updated` → `appStore.subscribe` обновляет
    `profiles`/`fetchedAt`/`nextUpdateAt`/`skippedCount` без перезагрузки окна.
  - `profiles_list` и `subscription_fetch` отдают `next_update_at`; UI на
    «Локациях» показывает «Следующее обновление: …» или «Автообновление
    выключено» (интервал `0`, теперь допустим в настройках).
- **Автоподключение при запуске (S4-T13, `1525df3`).**
  - `preferences.connect_on_start` применяется один раз при старте: `setup` →
    `autoconnect::spawn` → `spawn_blocking` → общая `connect_inner`. Окно
    helper'а не ждёт, `launch_minimized` не менялся.
  - Пропуск без сохранённой локации и при уже запущенном ядре.
  - Ошибка старта — `AppState.startup_error`, команда `autoconnect_status`,
    событие `autoconnect-failed`; снимается успешным `connect`/`disconnect`.
  - Слушатели событий в `App.tsx` ставятся **до** `initialize()`.
- **Подключение.** UI → `connect` → `build_config` → helper `start`; helper
  запускает sing-box sidecar и принимает только `status/start/stop`.
  **Протокол helper'а — версия 2.** `install`/`uninstall` по IPC запрещены.
- **Список запущенных процессов (S4-T11, `918c0c6`).** `RunningApp` в
  `crates/protocol`; macOS — `proc_listpids`/`proc_pidpath`, Linux —
  `/proc/<pid>/exe`, Windows — Toolhelp. Фильтр системных путей, лимит 500.
- **Пометка правил с несуществующим путём (S4-T12, `b5a1ade`).**
  `AppRuleView {…AppRule, missing_paths}`, локальный `stat` при чтении,
  `missing_paths` не сохраняется в `state.json`.
- **Задержки локаций и автовыбор (S4-T6, `2405d09`).** TCP-connect до
  `server:port`, 3 с, 64 профиля, 8 параллельно; гистерезис и подтверждения —
  в Rust; `profile_switch`: `stop` → выбор → `start` с тем же kill switch.
- **Kill switch (S4-T5a/T5b, opt-in fail-closed).** Протокол 2;
  `StartPayload {config, kill_switch}`, `HelperStatus.kill_switch`,
  `ErrorCode::KILL_SWITCH_FAILED`. macOS — pf anchor, Linux — nftables,
  Windows — WFP-провайдер и подслой.
- **Автообновление приложения (S4-T8, `5172dd5`).** Rust-only
  `tauri-plugin-updater`, каналы `stable`/`beta`, манифест — заглушка
  `.invalid` (`UPDATE_NOT_CONFIGURED` без сети).
- **Статистика трафика (S4-T4).** Локальный Clash API на `127.0.0.1`,
  случайный порт/secret на сессию; ошибка — «—» без `ErrorBox`.
- **Каталог и иконки (S4-T1/T2).** macOS 110 приложений / 100 иконок; Linux
  `.desktop`; Windows ярлыки/реестр.
- **Подписка (S4-T9/T10).** Happ-JSON, base64 и открытый текст `vless://`; UA по
  умолчанию `SplitRoute/0.1.0`, таймаут 45 с, `connect_timeout` 10 с.
- **Локализация (S4-T3).** `ru`/`en` в Rust, переключение без перезапуска.
- **Helper'ы и установщики.** macOS LaunchDaemon, Linux systemd/polkit, Windows
  служба + named pipe ACL. Установка LaunchDaemon blocked подписью.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1`.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **386 тестов** ok (1 `#[ignore]`).
- `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc
  --all-targets` — чисто.
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **98 тестов** ok;
  `pnpm build` ok.
- **S4-T14 (тесты):** 11 `autoupdate` — часы → секунды, `0` выключает,
  пустой URL без сети, свежая подписка не перезапрашивается, истёкший интервал
  обновляет кэш, ошибка сети сохраняет профили и `profiles_fetched_at` и
  переносит попытку на интервал, параллельная загрузка отклоняется и после
  освобождения проходит; 2 UI — время следующего обновления и «Автообновление
  выключено».
- **S4-T14 (live):** изолированный `HOME` + `SPLITROUTE_MOCK_HELPER=1` +
  локальный HTTP-сервер с фикстурой. `fetched_at` в прошлом, интервал 1 ч —
  ровно **1 запрос**, 2 профиля в кэше, `profiles_fetched_at` обновлён,
  `selected_profile_id` выставлен автоматически. Свежая подписка (интервал
  12 ч, `fetched_at = now − 60`) — **0 запросов**. После проверок процессы
  остановлены, `sing-box`/`splitroute` не остались, порт 1420 свободен.
- **S4-T13 (тесты):** 6 `autoconnect`, 1 UI; реальный запуск на изолированном
  `HOME` с `MockHelper` — окно открывается в состоянии «Подключено».
- **S4-T12 (тесты):** 10 `rules`, 3 protocol, 1 команда через store, 3 UI.
- **S4-T6 (тесты):** TCP-probe, стабильные коды, лимит 64, гистерезис,
  `stop → start`, восстановление прежней локации, сохранение kill switch.
- **sing-box check** и golden-тесты конфигов зелёные: конфиг ядра не менялся.
- **CI** (2026-11-14): `gh run` для `main` зелёный на `windows-latest` и
  `ubuntu-22.04` — сборка, `cargo test --workspace --locked`, фронтенд-проверки
  и пакеты (research §27).

### Не проверено (причина)

- **S4-T14 в реальном окне** — индикация проверена UI-тестами; клик по
  «Локациям» и визуальная проверка строки не автоматизировались.
- **S4-T14 с реальным провайдером** — нет стабильного ответа провайдера в
  этой сессии; сеть проверена локальным HTTP-сервером с фикстурой.
- **S4-T14 на Linux/Windows** — код платформонезависим, рантайма нет.
- **Фоновое обновление при закрытом приложении** — не реализовано осознанно:
  планировщик живёт только пока приложение запущено (PRD §6.1).
- **S4-T13 с установленным реальным helper'ом** — LaunchDaemon blocked
  подписью (S3-T2).
- **S4-T11 на Linux/Windows в рантайме** — ветки компилируются изолированно.
- **Реальная задержка серверов провайдера** — нет профиля/сети провайдера.
- **Реальное применение pf/nft/WFP под root** — `sudo -n` не проходит, нет
  Windows VM: **S4-T5c**.
- **Реальная установка обновления приложения** — нет публичного HTTPS-манифеста
  (PRD §13.2).
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
  идентификатор решить до публичной сборки (PRD §13.2).
- **Провайдер подписки:** формат зависит от UA; `SplitRoute/0.1.0` → base64
  `vless://` (~1 с), `Happ/4.1.0` → JSON (иногда ~17 с). Таймаут 45 с не уменьшать.
- **CachyOS доступна по SSH** (хост/каталог — в чате, не коммитить). Нет
  git-репозитория: синхронизация `rsync`. `sudo -n` требует пароль, GUI нет.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`). Секреты не логировать.
  Файл пишется только при изменениях: пустой handmade-`state.json` с неполным
  `network` не читается и приложение откатывается к значениям по умолчанию —
  для live-проверок правьте файл, который записало само приложение, или
  используйте полный `network`.
- **Автообновление подписки (S4-T14):** `src-tauri/src/autoupdate.rs`, событие
  `subscription-updated`, `next_update_at` в `profiles_list`. Для live-проверки
  `HOME=<tmp> RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo pnpm tauri dev`:
  без `RUSTUP_HOME`/`CARGO_HOME` rustup теряет toolchain.
- **Автоподключение (S4-T13):** `src-tauri/src/autoconnect.rs`; ошибка —
  `AppState.startup_error`, команда `autoconnect_status`, событие
  `autoconnect-failed`.
- **Clash API (S4-T4):** только `127.0.0.1`, случайный порт/secret на сессию;
  endpoint в `state.json` не сохраняется.
- **Kill switch runtime-файлы:** `<runtime>/killswitch.pf` (macOS),
  `killswitch.nft` (Linux), `killswitch.token` (macOS) — права `0600`.
- **Updater-ключ (S4-T8):** `~/.commandcode/splitroute-updater.key` и
  `.key.pass` (права `0600`), публичный — в `tauri.conf.json`. В git не
  коммитить. `SPLITROUTE_ALLOW_UNSIGNED=1` собирает без updater-артефактов.
- **Каталог процессов:** `src-tauri/src/catalog/running.rs`; macOS — локальная
  константа `PROC_ALL_PIDS = 1` (в `libc` её нет).
- **Скриншоты** с реальным URL/HWID/адресом сервера в репозиторий не коммитить.
- После проверок: `pgrep -x sing-box`, `pgrep -fl 'target/debug/splitroute'`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`,
  `launchctl print system/com.splitroute.helper`. Если `pnpm tauri dev` упал с
  «Port 1420 is already in use», это остаток прошлого прогона: убейте `node`
  на порту и `target/debug/splitroute` перед повтором.

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

### Автообновление подписки (S4-T14)

1. Настроить реальный URL подписки и интервал 1 ч, подключиться.
2. Перезапустить приложение: если интервал истёк, подписка обновится в фоне,
   список «Локаций» обновится без нажатия «Обновить подписку»; в строке под
   списком видно новое «Следующее обновление».
3. Поставить интервал `0`: строка показывает «Автообновление выключено», сеть
   не опрашивается (проверить по логам — нет строки «Автообновление подписки»).
4. Отключить сеть на время тика: подписка не пропадает, выбранная локация не
   меняется, в логах один `WARN`, следующая попытка — через интервал, не раньше.
5. Нажать «Обновить подписку», пока идёт фоновая загрузка: ошибки в UI нет,
   список обновится событием.

### Запущенные процессы на Linux/Windows (S4-T11)

Linux (есть машина по SSH):

1. `pnpm tauri dev` и открыть «Приложения → Запущенные».
2. Ожидается непустой список без systemd/DBus-демонов; «Добавить правило»
   создаёт выключенное правило с `process_path`.

### macOS: kill switch (S4-T5c)

1. `pnpm tauri dev` с реальным helper'ом (не MockHelper), dev-helper под `sudo`.
2. Включить kill switch, подключиться, пустить трафик выбранного приложения.
3. `sudo pkill -x sing-box`: прямой трафик блокируется, UI показывает ошибку.
4. Нажать «Отключить и снять защиту»: `pfctl -a com.apple/splitroute.killswitch
   -s rules` пуст, прямой трафик восстановлен.

### macOS: автоподключение при старте (S4-T13)

Шаги 1–2 проверены с `MockHelper`; с реальным helper'ом ждут подписи (S3-T2).

### Обновления приложения (S4-T8)

Выполнять после выбора домена и хостинга манифеста (PRD §13.2).

### Linux (S3-T3, S3-T7, S2-T9, S4-T5c)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
pnpm install && ./scripts/fetch-core.sh && ./scripts/package.sh
```

Проверить установку/`systemctl`/`pkexec`/TUN/per-app/DNS/kill switch/задержки по
матрице `docs/stage-3-report.md` §9.

### Windows (S3-T4, S4-T2, S4-T5b, S4-T5c)

`./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh`, затем
`./scripts/portable.sh` или `./scripts/package.sh`; служба под SYSTEM, UAC,
pipe ACL, `wintun.dll`, TUN/per-app, NSIS. WFP — только на VM/машине, где не
страшно потерять сеть.

---

## Промпт следующей подцели

Backlog исчерпан по текущему объёму MVP: следующих задач, готовых к работе без
root/сертификата/целевых ОС, нет. Варианты продолжения — выбрать один и
сформулировать цель по `TASKS.md`:

1. **Снять блокер S4-T5c / S3-T7** — ручной smoke kill switch и установки на
   реальной ОС (нужны root или Windows VM).
2. **S3-T2** — установка LaunchDaemon (нужен Apple Development/Developer ID).
3. **Новая фича из `Backlog`** — например, несколько подписок одновременно или
   импорт по QR; тогда сначала PRD, затем код (AGENTS §14.1).

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: <взять одну задачу из TASKS.md — блокер S4-T5c/S3-T7, S3-T2 или новую
фичу из Backlog>. Если пользователь в этой сессии назвал другую задачу —
выполняй её. После одной подцели остановись: коммит кода/тестов, отдельный
коммит документации, обновление NEXT_SESSION.md. Следующие задачи не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §10, §12–§14, §17–§18.
- PRD.md: раздел по выбранной задаче (§6.1, §6.2, §6.6 — для подписки/сети;
  §13 — принятые решения).
- TASKS.md: статус выбранной задачи и связанные ID.
- docs/research.md: релевантные разделы (§19–§28 — этап 4).
- NEXT_SESSION.md — состояние, что проверено, что blocked.

ТЕКУЩЕЕ СОСТОЯНИЕ
- Этап 4: S4-T1…S4-T14 done. S4-T5c blocked (root/Windows VM), S3-T2 blocked
  (сертификат Apple), S3-T7 blocked частично (Linux/Windows-runtime).
  Последний коммит кода — `168a089` (автообновление подписки).
- Проверено 2026-11-14: 386 Rust-тестов (1 ignored), 98 frontend,
  fmt/clippy/typecheck/lint/build зелёные, live-проверка планировщика подписки.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- Новых привилегий и shell-команд; helper и его протокол не меняются без
  отдельного решения в PRD.
- `crates/protocol` — единственный источник общих моделей; URL подписки, UUID и
  адреса серверов не попадают в логи, фикстуры и ответы UI.
- Загрузка не блокирует окно (`async`/фон). Ошибка сети не стирает кэш
  последнего успешного ответа (S4-T9/S4-T10) и не подменяет выбранную локацию.
- Ручное «обновить сейчас», автоподключение (S4-T13) и kill switch не
  затрагиваются без явной необходимости.
- Не добавлять новых зависимостей без обоснования (AGENTS §3).

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc`
- `pnpm tauri dev` (MockHelper) не должен ломаться; после проверки остановить
  и убедиться, что `sing-box`, `target/debug/splitroute` и порт 1420 свободны.
- Если менялся код CI-пути — после пуша проверить `gh run list`/`gh run watch`.

ДОКУМЕНТАЦИЯ
- TASKS.md: статус задачи и ссылка на коммит.
- PRD.md: только если меняется поведение продукта.
- docs/research.md: что проверено и ограничения (новый раздел).
- NEXT_SESSION.md: состояние и самодостаточный промпт следующей подцели.

ОГРАНИЧЕНИЯ
- Не менять helper-протокол, kill switch, обновления приложения, установщики,
  иконки и статистику сверх задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не оставлять dev-серверы и процессы после проверок.

ОТЧЁТ
- TASKS.md: статус со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, промпт следующей подцели.
- Коммиты: feat:/fix:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S4-T14 (автообновление подписки):** логика — `src-tauri/src/autoupdate.rs`,
  вызов — `setup` в `lib.rs`. Не запускайте планировщик чаще раза в минуту и не
  снимайте `FetchGuard` перед загрузкой: параллельная загрузка с ручной кнопкой
  — единственный способ получить два запроса к провайдеру и два конкурентных
  `set_profiles`. Попытку запоминайте и при ошибке, иначе недоступный провайдер
  будет опрашиваться на каждом тике. `next_update_at` — производное значение:
  не храните его в `state.json`.
- **S4-T13 (автоподключение):** логика — `src-tauri/src/autoconnect.rs`.
  Не убирайте подписку на `autoconnect-failed` и не меняйте порядок «сначала
  слушатели, потом `initialize()`» в `App.tsx`.
- **S4-T12 (пометка «неактивно»):** логика — `src-tauri/src/rules.rs`;
  проверяются только абсолютные `process_path` и `path_regex`, сводящиеся к
  литеральному пути. Не расширяйте проверку на `process_name` и regex с
  метасимволами: ложное «неактивно» хуже отсутствия пометки.
- **S4-T11 (запущенные процессы):** macOS — `proc_listpids`/`proc_pidpath`;
  Linux — `/proc/<pid>/exe`; Windows — Toolhelp. Внешних `ps`/`tasklist` не
  добавлять. Иконки в «Запущенных» не запрашивать.
- **S4-T6 (задержки/автовыбор):** замер — TCP-connect, не прокси-запрос.
  Переключение — `stop → start`: kill switch на мгновение снимается.
- **S4-T8 (обновления приложения):** endpoint `.invalid` — заглушка; не
  подменять публичным URL без решения PRD §13.2. Приватный ключ не коммитить.
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
