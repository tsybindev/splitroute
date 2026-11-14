# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS GUI/root).
- **Этап 3:** S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично**
  (Linux-пакеты собраны, установка/`systemctl`/TUN/GUI не проверены; Windows VM нет).
- **Этап 4:** S4-T1…S4-T6, S4-T9, S4-T10 — done.
  **S4-T5b — todo (следующая)**. **S4-T5c — blocked** (ручная проверка под root).
  **S4-T7 — todo, ждёт решения PRD §13.2**. **S4-T8 — todo**.
- Дата проверки: **2026-11-14**. Ветка `main`. Последний коммит кода — `2405d09`
  (feat: замер задержек и автопереключение локации). Документация — отдельный
  коммит после него.
- Факты: [docs/research.md](./docs/research.md) §19 (статистика), §20 (kill
  switch), §21 (задержки и автопереключение).

### Что работает

- **Подключение.** UI → `connect` → `build_config` → helper `start`; helper
  запускает sing-box sidecar и принимает только `status/start/stop`.
  **Протокол helper'а — версия 2.** `install`/`uninstall` по IPC запрещены.
  Установленный helper v1 несовместим: нужна переустановка из настроек.
- **Задержки локаций и автовыбор (S4-T6, коммит `2405d09`).**
  - Замер — TCP-connect до `server:port` в непривилегированном приложении:
    ядро не запускается, helper не участвует, UUID/Reality-ключи не читаются.
    3 с на одно TCP-подключение, лимит 64 профиля, 8 параллельных замеров,
    `#[tauri::command(async)]`. Ошибки — `PING_TIMEOUT`, `PING_UNREACHABLE`,
    `PING_DNS`.
  - `profiles_ping` возвращает `results[]` (id, мс/ошибка), `best_profile_id`
    (минимум, при равенстве — первый в подписке) и `switch_to` — рекомендацию
    автопереключения. В логи попадают только имена локаций и числа.
  - Настройки `preferences.auto_select`: `enabled`, `auto_switch`,
    `degradation_ms` (100), `interval_secs` (60), `confirmations` (2),
    `failures_to_switch` (2). По умолчанию всё выключено, старые `state.json`
    читаются через `#[serde(default)]`, диапазоны нормализуются при сохранении.
  - Решение о переключении — в Rust: гистерезис (улучшение не меньше порога),
    несколько подтверждений подряд и серия неудачных замеров текущей локации.
    UI только опрашивает `profiles_ping` по интервалу и вызывает
    `profile_switch`.
  - `profile_switch`: `stop` → выбор → `start` с тем же флагом kill switch.
    Без туннеля только сохраняет выбор. При ошибке старта возвращает прежнюю
    локацию, чтобы не оставить сеть без туннеля и правил.
  - UI: кнопка «Проверить задержки», мс/ошибка, сортировка по задержке и
    «Лучшая» в «Локациях»; карточка автовыбора на «Главной»; группа настроек с
    подсказками; строки `ru`/`en`.
- **Kill switch (S4-T5a, opt-in fail-closed).**
  - `Settings.network.kill_switch` (default `false`, старые `state.json` → `false`).
  - Протокол 2: `StartPayload {config, kill_switch}`, `HelperStatus.kill_switch
    {enabled, error?}`, `ErrorCode::KILL_SWITCH_FAILED`.
  - Helper ставит правила **до** запуска ядра. При падении ядра правила остаются
    (fail-closed); снимаются при `stop`, выходе приложения (`RunEvent::Exit` →
    `stop`) или выключении настройки. Старт helper'а очищает «залипшие» правила
    после захвата сокета. Неудачный старт снимает только правила, поставленные
    этим вызовом.
  - Правила разрешают loopback, локальные сети (`10/8`, `172.16/12`,
    `192.168/16`, `169.254/16`, multicast, IPv6 link-local/ULA/multicast) и
    трафик root (ядра), остальное блокируют. macOS — pf anchor
    `com.apple/splitroute.killswitch` (`/etc/pf.conf` уже содержит
    `anchor "com.apple/*"`; `pfctl -E` + токен в runtime-каталоге `0600`),
    Linux — таблица nftables `inet splitroute_killswitch`. Windows возвращает
    `KILL_SWITCH_FAILED` (`S4-T5b`).
- **Статистика трафика (S4-T4).** Локальный Clash API на `127.0.0.1`, случайный
  порт/secret на сессию, `GET /connections` с Bearer; ошибка — «—» без `ErrorBox`.
- **Подписка.** Happ-JSON, base64 и открытый текст `vless://`; UA по умолчанию
  `SplitRoute/0.1.0`, таймаут 45 с, `connect_timeout` 10 с.
- **Каталог и иконки (S4-T1/T2).** macOS 110 приложений / 100 иконок; Linux
  `.desktop`; Windows ярлыки/реестр. Иконка — отдельная `app_icon(app)`.
- **Локализация (S4-T3).** `ru`/`en` в Rust, переключение без перезапуска.
- **Helper'ы и установщики.** macOS LaunchDaemon, Linux systemd/polkit, Windows
  служба + named pipe ACL. Установка LaunchDaemon blocked подписью.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1`.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **321 тест** ok (1 `#[ignore]`).
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **78 тестов** ok;
  `pnpm build` ok.
- `pnpm tauri dev` с `SPLITROUTE_MOCK_HELPER=1`: окно `SplitRoute` открывается,
  процесс `target/debug/splitroute` работает; после проверки остановлен, порт
  1420 свободен, `sing-box`/`splitroute` не остались.
- **S4-T6 (тесты):** TCP-probe на loopback (открытый/закрытый порт, плохое
  имя), стабильность кодов `PING_*`, минимум и порядок при равенстве, лимит
  64 профиля, гистерезис и подтверждения, серия неудач, отсутствие рабочей
  альтернативы, сброс трекера, `stop → start` при переключении, выбор без
  туннеля, восстановление прежней локации при ошибке старта, сохранение флага
  kill switch, отсутствие адресов серверов в логах, миграция старых настроек и
  нормализация диапазонов, UI-кнопка/сортировка/«Лучшая»/ошибка,
  автопереключение из UI.
- **sing-box check** и golden-тесты конфигов зелёные: конфиг ядра не менялся.
- **Kill switch (тесты):** тексты правил pf/nft, идемпотентность enable/disable,
  ошибка enable → `KILL_SWITCH_FAILED` без записи конфига, сохранение правил
  после «падения», `stop` снимает правила, serde протокола 2, mock helper,
  команда `connect` передаёт флаг, UI переключатель/состояния/ошибка.

### Не проверено (причина)

- **Реальная задержка серверов провайдера** — в сессии нет реального профиля и
  сети провайдера; тесты используют fake-probe и закрытые порты.
- **Замер на Linux/Windows** — код платформонезависим (`std::net`), но рантайм
  не запускался.
- **DNS-резолв не имеет отдельного таймаута** — `to_socket_addrs` использует
  системный resolver; при зависшем resolver проверка может быть дольше 3 с.
- **Окно между `stop` и `start` при переключении** — правила kill switch на это
  время снимаются; при ошибке старта приложение возвращает прежнюю локацию, но
  абсолютной атомарности без нового протокола helper'а нет.
- **Реальное применение pf/nft под root** — `sudo -n true` не проходит; нужна
  ручная проверка `S4-T5c`.
- **Windows kill switch** — не реализован (`S4-T5b`); включение даёт
  `KILL_SWITCH_FAILED`.
- **nftables на Linux** — нет машины с root/GUI; правило и классификация ошибок
  покрыты тестами, рантайм — нет.
- **Сон/пробуждение и смена сети с активным kill switch** — правила не содержат
  адресов, но рантайм не проверялся.
- **Статистика после перезапуска окна** — «—» до переподключения (осознанно).
- **Установка/запуск LaunchDaemon** — blocked: нужен Apple Development/Developer ID.
- **TUN, per-app, DNS, сон/пробуждение на упакованных бинарях** — blocked: нет
  root и реальной подписки в сессии.
- **Linux-рантайм** (S2-T9, S3-T3, S3-T7) — нет GUI/пароля sudo.
- **Windows-рантайм** (S3-T4, S4-T2, S4-T5b) — нет VM; полный
  `cargo check -p splitroute --target x86_64-pc-windows-msvc` падает на `ring`.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`, Linux —
  `gio-sys`/`glib-sys`. Платформенные ветки проверяйте изолированными крейтами.
- `sudo -n true` **не проходит**; привилегированные действия — только с явным
  подтверждением пользователя.
- **Debug-сборка** ищет сокет в `$TMPDIR/splitroute-helper.sock`. Установленный
  helper: macOS — `/var/run/splitroute/helper.sock`, Linux —
  `/run/splitroute/helper.sock`, Windows — `\\.\pipe\splitroute-helper`;
  переопределение — `SPLITROUTE_HELPER_SOCKET`/`SPLITROUTE_HELPER_PIPE`.
- **Helper: одно сообщение на соединение.** Smoke-клиент подключается заново.
- **Установленный helper v1 несовместим с протоколом 2** — после обновления
  бинарей переустановите helper; иначе `HELPER_PROTOCOL`.
- **Bundle id** `com.splitroute.app` заканчивается на `.app` — Tauri
  предупреждает; финальный идентификатор решить до публичной сборки (PRD §13.3).
- **Провайдер подписки:** формат зависит от UA; `SplitRoute/0.1.0` → base64
  `vless://` (~1 с), `Happ/4.1.0` → JSON (иногда ~17 с). Таймаут 45 с не уменьшать.
- **CachyOS доступна по SSH** (хост/каталог — в чате, не коммитить). Нет
  git-репозитория: синхронизация `rsync`. `sudo -n` требует пароль, GUI нет.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`). Секреты не логировать.
- **Автовыбор в state.json:** `preferences.auto_select`; старые файлы читаются
  с выключенными функциями (100 мс, 60 с, 2 подтверждения, 2 неудачи).
- **Clash API (S4-T4):** только `127.0.0.1`, случайный порт/secret на сессию;
  endpoint в `state.json` не сохраняется; в логи/диагностику не попадает.
- **Kill switch runtime-файлы:** `<runtime>/killswitch.pf` (macOS),
  `killswitch.nft` (Linux), `killswitch.token` (macOS) — права `0600`, удаляются
  при снятии правил.
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

### macOS: kill switch (S4-T5c)

1. `pnpm tauri dev` с реальным helper'ом (не MockHelper), dev-helper под `sudo`
   — см. AGENTS §4.5; включить kill switch в настройках.
2. Подключиться, выбрать приложение с правилом, запустить в нём трафик.
3. `sudo pkill -x sing-box`. Ожидается: прямой трафик выбранного приложения
   блокируется, UI показывает «Ошибка» и кнопку «Отключить и снять защиту».
4. Нажать кнопку (или «Отключить»). Ожидается: `pfctl -a
   com.apple/splitroute.killswitch -s rules` пуст, правила сняты, прямой трафик
   восстановлен, `pfctl -s references` не содержит ссылки helper'а.
5. Проверить, что после перезапуска helper'а «залипших» правил нет.

### macOS: задержки и автопереключение (S4-T6)

1. С реальным профилем нажать «Проверить задержки»: числа должны быть
   сопоставимы с `ping`/`nc -z` до тех же серверов; ошибки — локализованные.
2. Включить автовыбор, обновить подписку — выбранная локация должна стать
   самой быстрой по TCP.
3. Включить автопереключение, подключиться, дождаться двух проверок: при
   деградации выше порога должен произойти `stop → start` без зависшего TUN.
4. При ошибке старта новой локации проверить, что вернулась прежняя и правила
   kill switch снова применены.

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

### Windows (S3-T4, S4-T2, S4-T5b)

`./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh`, затем
`./scripts/portable.sh` или `./scripts/package.sh`; проверить службу под SYSTEM,
UAC, pipe ACL, `wintun.dll`, TUN/per-app, NSIS. Kill switch пока не реализован.

---

## Промпт следующей подцели: G4.7 (S4-T5b, kill switch на Windows)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.7 этапа 4 — S4-T5b «Kill switch на Windows (WFP/firewall)».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §6–§7, §10, §12–§14, §17–§18.
- PRD.md §6.2 (kill switch), §6.6, §9, §10 (R13/R14), §13.
- TASKS.md: S4-T5b, S4-T5c и этап 4; docs/research.md §20 (kill switch) и §21
  (задержки/автовыбор — это соседняя закрытая задача, не ломай её).
- NEXT_SESSION.md, crates/protocol/src/lib.rs, crates/helper/src/
  {core.rs,handler.rs,killswitch/mod.rs,killswitch/rules.rs,macos.rs,linux.rs},
  src-tauri/src/privileged/*, src-tauri/src/commands/mod.rs.

ТЕКУЩЕЕ СОСТОЯНИЕ
- S4-T1…S4-T6, S4-T9, S4-T10 закрыты. Последний коммит кода — `2405d09`
  (замер задержек и автопереключение локации).
- Протокол helper'а — версия 2: `StartPayload.kill_switch`,
  `HelperStatus.kill_switch {enabled,error}`, `ErrorCode::KILL_SWITCH_FAILED`.
  Менять протокол для S4-T5b не нужно: флаг и статус уже есть.
- `KillSwitch` и трейт `Firewall` уже платформенно-нейтральны; macOS pf и Linux
  nftables реализованы. На Windows `platform_firewall()` возвращает
  `UnsupportedFirewall`, `enable()` даёт ошибку, `disable()` — Ok.
- Проверено 2026-11-14: 321 Rust-тест, 78 frontend, clippy/typecheck/lint/build
  зелёные, `pnpm tauri dev` (MockHelper) открывает окно.
- Нет Windows VM; полный `cargo check -p splitroute --target
  x86_64-pc-windows-msvc` падает на `ring`. `cargo check -p splitroute-helper
  --target x86_64-pc-windows-msvc` использовать можно.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- `crates/protocol` — единственный источник общих типов. В S4-T5b протокол не
  менять: клиент уже передаёт `kill_switch`, helper уже возвращает состояние.
- Минимальная привилегированная поверхность: никаких данных от клиента в
  правилах, никаких произвольных путей/команд по IPC. Правила фиксированы в
  helper'е; backend получает только runtime-каталог.
- `Firewall` должен остаться идемпотентным: `enable()`/`disable()` можно звать
  повторно; ошибка `disable()` оставляет `enabled=true` и `error`, как на
  macOS/Linux.
- Ошибки — безопасный русский текст без секретов и без вывода сырых команд.
- Новых тяжёлых зависимостей не добавлять без обоснования. Комментарии — на
  русском, идентификаторы — английские.

ЗАДАЧИ
1. Решение и PRD (до кода).
   - Выбери механизм: WFP-фильтры через `windows-sys`/`windows` или fixed
     `netsh advfirewall`-команды без данных от клиента. Обоснуй минимальный
     вариант, права (helper под SYSTEM), идемпотентное снятие, поведение при
     падении ядра и `stop`.
   - Зафиксируй, что именно разрешается: loopback, локальные сети, трафик
     SYSTEM/ядра (`sing-box.exe`), остальное блокируется. Windows не имеет uid
     root — явно опиши эквивалент исключения.
   - Обнови PRD §6.2/§6.6/§10 (R13/R14) и TASKS.md. Если объём не влезает —
     разбей S4-T5b на подзадачи и реализуй первую проверяемую часть.
2. Реализация.
   - `crates/helper/src/killswitch/windows.rs`: `SystemFirewall` с тем же
     трейтом; подключи его в `platform_firewall()` под `cfg(target_os =
     "windows")`. Убери/сузь `UnsupportedFirewall` так, чтобы на Windows он не
     использовался.
   - Правила/команды — статические константы, тестируемые на macOS (как
     `rules.rs` для pf/nft). Никаких строк от клиента.
   - Не меняй `KillSwitch`-логику и протокол; `core.rs` должен продолжить
     ставить правила до запуска ядра и сохранять их при падении.
3. Тесты.
   - Чистые функции: текст/список правил, порядок исключений и блока,
     отсутствие подстановок, идемпотентность, ошибка enable/disable.
   - `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc` и
     `cargo clippy` для helper под эту цель; если что-то нельзя проверить на
     macOS — честно записать.
   - Существующие тесты macOS/Linux не должны сломаться.
4. Проверки.
   - cargo fmt --all --check
   - cargo clippy --workspace --all-targets -- -D warnings
   - cargo test --workspace
   - pnpm typecheck && pnpm lint && pnpm test && pnpm build
   - `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc`
   - `pnpm tauri dev` (MockHelper) не должен ломаться.
   - Реальной Windows VM нет — не выдавай непроверенное за проверенное.
5. Документация.
   - TASKS.md: S4-T5b → done (или подзадачи) со ссылкой на коммит.
   - docs/research.md: проверенные факты, ограничения и что осталось на S4-T5c.
   - NEXT_SESSION.md: состояние и промпт следующей незакрытой задачи
     (S4-T7 доменные правила после решения PRD §13.2 или S4-T8 — по статусу).

ОГРАНИЧЕНИЯ
- Не менять helper-протокол, установщики, иконки, локализацию, статистику и
  автовыбор/задержки сверх задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не оставлять «залипших» процессов, TUN, правил kill switch и dev-серверов.

ОТЧЁТ
- TASKS.md: статус S4-T5b со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, самодостаточный промпт следующей
  подцели.
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S4-T6 (задержки/автовыбор):** замер — TCP-connect, не прокси-запрос; в UI
  это подписано. DNS-резолв отдельным таймаутом не ограничен. Переключение —
  `stop → start`, поэтому kill switch на мгновение снимается; при ошибке старта
  возвращается прежняя локация. Не добавляйте в замер ядро или helper.
- **S4-T5b (Windows):** включение сейчас возвращает `KILL_SWITCH_FAILED`, ядро
  не стартует. Не «включайте» защиту без реальных правил.
- **Kill switch (S4-T5a):** протокол 2; после обновления переустановите helper.
  Правила шире allowlist — при мёртвом туннеле прямой выход теряют все
  непривилегированные процессы; локальная сеть остаётся. Снятие — `stop`,
  выход приложения или выключение настройки. Не добавляйте в правила адреса,
  порты или данные от клиента.
- **S4-T4 (статистика):** источник — локальный Clash API; порт/secret в памяти,
  после перезапуска окна — «—» до переподключения.
- **S4-T3 (локализация):** новые строки — в `ru.ts` и `en.ts`; тест запрещает
  кириллицу в `en`. Язык меняется через `settings_update`.
- **S4-T2 (иконки):** не переносите чтение иконок в `apps_installed`.
- **S4-T1 (каталог):** macOS-скан не рекурсивный; «Запущенные» — заглушка.
- **S3-T2 blocked подписью:** нужен Apple Development/Developer ID.
- **Лицензии:** при смене sing-box/wintun править `THIRD_PARTY_NOTICES`;
  тест `packaging_config` это проверяет.
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
- **Секреты** (URL подписки, HWID, UUID, реальные адреса, `secret` Clash API,
  содержимое pf/nft-правил с адресами) в задачи, фикстуры, логи и коммиты не
  попадают.
