# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS GUI/root).
- **Этап 3:** S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично**
  (Linux-пакеты собраны, установка/`systemctl`/TUN/GUI не проверены; Windows VM нет).
- **Этап 4:** S4-T1, S4-T2, S4-T3, S4-T4, **S4-T5a**, S4-T9, S4-T10 — done.
  **S4-T5b — todo** (kill switch на Windows). **S4-T5c — blocked** (ручная
  проверка под root). Следующая — **S4-T6 (автовыбор и автопереключение локации)**.
- Дата проверки: **2026-11-14**. Ветка `main`. Последний коммит кода — `2405144`
  (feat: kill switch fail-closed на macOS и Linux).
- Факты: [docs/research.md](./docs/research.md) §19 (статистика), §20 (kill switch).

### Что работает

- **Подключение.** UI → `connect` → `build_config` → helper `start`; helper
  запускает sing-box sidecar и принимает только `status/start/stop`.
  **Протокол helper'а — версия 2.** `install`/`uninstall` по IPC запрещены.
  Установленный helper v1 несовместим: нужна переустановка из настроек.
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
  - UI: переключатель с предупреждением в «Настройках», карточка
    активен/выключен/ошибка на «Главной», при падении ядра — `CORE_STOPPED` и
    кнопка «Отключить и снять защиту».
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
  — чисто. `cargo test --workspace` — **302 теста** ok (1 `#[ignore]`).
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **70 тестов** ok;
  `pnpm build` ok.
- `pnpm tauri dev` с `SPLITROUTE_MOCK_HELPER=1`: окно `SplitRoute` открывается,
  процесс `target/debug/splitroute` работает; после проверки остановлен, порт
  1420 свободен.
- **Kill switch (тесты):** тексты правил pf/nft (порядок исключений и блока,
  отсутствие подстановок), идемпотентность enable/disable, ошибка enable →
  `KILL_SWITCH_FAILED` без записи конфига, ошибка disable → `enabled=true` +
  `error`, откат только новых правил, сохранение правил после «падения»,
  `stop` снимает правила, serde протокола 2, mock helper, команда `connect`
  передаёт флаг, UI переключатель/состояния/ошибка.
- **pf.conf macOS 15.8.2** (только чтение): содержит
  `anchor "com.apple/*"` и `load anchor "com.apple"`, поэтому дочерний anchor
  применяется без правки системного файла. `pfctl -s info` без root →
  `Permission denied`.

### Не проверено (причина)

- **Реальное применение pf/nft под root** — `sudo -n true` не проходит; нужна
  ручная проверка `S4-T5c`: подключиться, убить ядро, убедиться, что прямой
  выход заблокирован, после `disconnect` правила сняты.
- **Windows kill switch** — не реализован (`S4-T5b`); включение даёт
  `KILL_SWITCH_FAILED`.
- **nftables на Linux** — нет машины с root/GUI; правило и классификация ошибок
  покрыты тестами, рантайм — нет.
- **Сон/пробуждение и смена сети с активным kill switch** — правила не содержат
  адресов, но рантайм не проверялся.
- **Kill switch и `SIGKILL` helper'а** — правила остаются до следующего старта
  helper'а (он их очищает) или ручного `pfctl`/`nft`; это fail-closed.
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

### macOS: реальные счётчики трафика (S4-T4)

Повторить сценарий из прошлого handoff: ↑/↓ растут, после `stop` — «—», после
нового `start` — с нуля; после `pkill -x sing-box` — ровно один `Warn`.

### macOS: LaunchDaemon (S3-T2)

Нужен сертификат Apple Development/Developer ID; ad-hoc не подходит (AMFI:
`has no CMS blob`). Чек-лист — `docs/stage-3-report.md` §6.

### Linux (S3-T3, S3-T7, S2-T9, S4-T5c)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
pnpm install
./scripts/fetch-core.sh
./scripts/package.sh             # deb, rpm, appimage
```

Проверить установку/`systemctl`/`pkexec`/TUN/per-app/DNS/kill switch по матрице
`docs/stage-3-report.md` §9. AppImage без FUSE:
`./SplitRoute_*.AppImage --appimage-extract-and-run`.

### Windows (S3-T4, S4-T2, S4-T5b)

`./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh`, затем
`./scripts/portable.sh` или `./scripts/package.sh`; проверить службу под SYSTEM,
UAC, pipe ACL, `wintun.dll`, TUN/per-app, NSIS. Kill switch пока не реализован.

---

## Промпт следующей подцели: G4.6 (S4-T6, автовыбор локации)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.6 этапа 4 — S4-T6 «Автовыбор и автопереключение локации».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3, §6–§7, §10, §12–§14, §17–§18.
- PRD.md §6.1, §6.2, §6.6, §7, §8, §10, §13.
- TASKS.md: S4-T6 и этап 4; docs/research.md §19–§20 (текущие connect/status,
  статистика и kill switch) и §16 (таймауты подписки).
- NEXT_SESSION.md, src-tauri/src/{commands/mod.rs,config/mod.rs,store/mod.rs,
  privileged/*,traffic/*}, crates/protocol/src/lib.rs,
  src/features/{connect,profiles,settings}/, src/shared/{api,state,i18n}/,
  src-tauri/tests/.

ТЕКУЩЕЕ СОСТОЯНИЕ
- S4-T1…S4-T5a, S4-T9, S4-T10 закрыты. Коммит S4-T5a — `2405144`.
- Протокол helper'а — версия 2 (`StartPayload.kill_switch`,
  `HelperStatus.kill_switch`). Helper принимает только `status/start/stop`;
  `install`/`uninstall` по IPC запрещены.
- Подключение: UI → `connect` → `build_config` → helper `start`; `AppState`
  хранит профиль, правила и endpoint статистики в памяти.
- Измерения задержки локаций в коде отсутствуют (M2). Есть `reqwest` (rustls),
  `tokio`; sing-box 1.14.4 с `with_clash_api` (loopback + secret) и Clash API
  `/connections`.
- Проверено 2026-11-14: 302 Rust + 70 frontend тестов, clippy/typecheck/lint/
  build зелёные, `pnpm tauri dev` (MockHelper) открывает окно.
- `sudo -n true` не проходит: привилегированные проверки — только с явным
  участием пользователя. Нет Windows VM; CachyOS по SSH без GUI.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- `crates/protocol` — единственный источник общих типов. Менять протокол
  helper'а можно только осознанно: поднять `PROTOCOL_VERSION`, обновить обе
  стороны, тесты сериализации/диспетчера и проверку несовместимости, обновить
  PRD. Если задача решается без helper'а — протокол не трогать.
- Минимальная привилегированная поверхность helper'а: никаких `exec`, shell,
  произвольных путей и правил от клиента. Задержку меряет непривилегированное
  приложение, а не helper.
- UI не получает root, не вызывает shell и не обращается к ядру в обход
  Tauri-команд. Конфиг — данные, не код; локальные API — только `127.0.0.1`.
- Никакой телеметрии. Секреты, URL, HWID, UUID и адреса серверов не логировать
  и не коммитить. В логах — только имена локаций и числа задержек.
- Новых тяжёлых зависимостей не добавлять без обоснования. Комментарии — на
  русском, идентификаторы — английские.

ЗАДАЧИ
1. Решение и PRD (до кода).
   - Определи, как меряется задержка: TCP-connect до `server:port` без секретов
     и без запуска ядра либо Clash API `/proxies/{tag}/delay` уже подключённого
     профиля. Обоснуй минимальный вариант, таймауты, параллельность и лимиты;
     не блокируй главный поток (`#[tauri::command(async)]`).
   - Определи автовыбор: по минимальной задержке среди профилей; порог
     деградации, гистерезис, число неудачных замеров, поведение при обрыве
     туннеля и при активном kill switch (переключение не должно оставлять
     TUN/правила в неожиданном состоянии).
   - Определи пользовательские настройки: включить/выключить автовыбор
     и автопереключение, порог/интервал; значения по умолчанию; миграция
     старых `state.json` (`#[serde(default)]`).
   - Обнови PRD §6.1/§6.2/§6.6/§7/§10 и TASKS.md. Если объём не влезает —
     разбей S4-T6 на подзадачи и реализуй первую проверяемую часть.
2. Реализация.
   - Rust: модуль замера задержек, чистая функция выбора лучшей локации,
     состояние и таймауты; команда `profiles_ping` (или аналог) для UI;
     сохранение выбранной локации через существующий store.
   - Автопереключение — только по явному решению; не переключай без сети,
     не снимай kill switch и не оставляй TUN после `stop`/`start`.
   - UI: кнопка «проверить задержки», отображение мс/ошибки в «Локациях»,
     переключатели и подсказки в «Настройках», состояние на «Главной».
     Строки сразу в `ru` и `en`; ошибки — типизированными кодами.
3. Тесты.
   - Rust: парсинг/валидация задержек, выбор минимума, гистерезис/порог,
     ошибки сети → типизированный код, отсутствие секретов в логах,
     `connect` не ломается.
   - Frontend: кнопка/состояния, сортировка по задержке, переключатели,
     совпадение ключей `ru`/`en`.
   - `sing-box check` для конфигов должен оставаться зелёным.
4. Проверки.
   - cargo fmt --all --check
   - cargo clippy --workspace --all-targets -- -D warnings
   - cargo test --workspace
   - pnpm typecheck && pnpm lint && pnpm test && pnpm build
   - `pnpm tauri dev` (MockHelper) не должен ломаться.
   - Реальный провайдер/root недоступны — не выдавай непроверенное за проверенное.
5. Документация.
   - TASKS.md: S4-T6 → done (или подзадачи) со ссылкой на коммит.
   - docs/research.md: проверенные факты и ограничения замера.
   - NEXT_SESSION.md: состояние и промпт следующей незакрытой задачи
     (S4-T7 доменные правила, S4-T5b Windows kill switch или S4-T8 — по статусу).

ОГРАНИЧЕНИЯ
- Не менять helper-протокол, установщики, иконки, локализацию и статистику сверх
  задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не оставлять «залипших» процессов, TUN, правил kill switch и dev-серверов.

ОТЧЁТ
- TASKS.md: статус S4-T6 со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, самодостаточный промпт следующей
  подцели.
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **Kill switch (S4-T5a):** протокол 2; после обновления переустановите helper.
  Правила шире allowlist — при мёртвом туннеле прямой выход теряют все
  непривилегированные процессы; локальная сеть остаётся. Снятие — `stop`,
  выход приложения или выключение настройки. Не добавляйте в правила адреса,
  порты или данные от клиента.
- **S4-T5b (Windows):** включение сейчас возвращает `KILL_SWITCH_FAILED`, ядро
  не стартует. Не «включайте» защиту без реальных правил.
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
