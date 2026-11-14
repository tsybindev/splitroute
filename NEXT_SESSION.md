# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS).
- **Этап 3: S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.**
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично:**
  macOS-часть выполнена; на CachyOS собраны `.deb`/`.rpm`/AppImage и прогнаны
  тесты, но установка, `systemctl`/`pkexec`, TUN и GUI не проверены;
  Windows VM нет.
- **Этап 4: S4-T1, S4-T2, S4-T3, S4-T4, S4-T9, S4-T10 — done.** Следующая —
  **S4-T5 (kill switch, fail-closed)**.
- Дата проверки: **2026-11-14**. Ветка `main`. Последний коммит кода — `cda798e`
  (feat: счётчики трафика сессии из Clash API); до него `613a27e` (EN-локализация),
  `9fd1bdf` (base64/vless), `06701bf` (зависание подписки). Факты:
  [docs/research.md](./docs/research.md) §18–§19.

### Что работает

- **Helper, оба транспорта.** `crates/helper`: Unix-сокет `0600` (uid пира
  `getpeereid`/`SO_PEERCRED`) и Windows named pipe
  `\\.\pipe\splitroute-helper` с SDDL-ACL на SID пользователя, SYSTEM и
  Administrators + сверка SID клиента. Протокол: `status`/`start`/`stop`;
  `install`/`uninstall` по IPC запрещены. **Одно сообщение на соединение** —
  клиент переподключается перед каждой командой.
- **Установщики.** macOS (`/Library/PrivilegedHelperTools`, LaunchDaemon,
  `osascript`), Linux (`/usr/lib/splitroute`, systemd-юнит `0711`/`UMask=0077`,
  `pkexec`), Windows (`C:\Program Files\SplitRoute`, служба под SYSTEM,
  named pipe ACL, `icacls`, UAC). Коды `INSTALLER_*` показываются с точной
  причиной.
- **Подписка.** `subscription_fetch` — `#[tauri::command(async)]`; формат тела
  определяется автоматически: Happ-JSON, base64 или открытый текст `vless://`
  (Reality/TLS, `pbk`/`sid`/`sni`/`fp`/`alpn`, `flow`, TCP, IPv6). UA по
  умолчанию — `SplitRoute/0.1.0`; сохранённый UA не перезаписывается. Таймаут
  45 с, `connect_timeout` 10 с.
- **Каталог и иконки (S4-T1, S4-T2).** `apps_installed` и отдельная
  `app_icon(app)` (`#[tauri::command(async)]`); macOS 110 приложений / 100
  иконок, Linux — `.desktop` + `hicolor`/`pixmaps`, Windows — ярлыки и реестр;
  нормализация до 64 px, кэш в памяти, заглушка без `ErrorBox`.
- **Локализация (S4-T3).** Язык `ru`/`en` — в Rust
  (`AppSettings.preferences.language`), все видимые строки в
  `src/shared/i18n/{ru,en}.ts`, переключение без перезапуска, меню трея
  локализовано на стороне Rust.
- **Статистика трафика (S4-T4).** Новый модуль `src-tauri/src/traffic/`:
  - `build_config` получает `Option<&ClashApi>` и добавляет
    `experimental.clash_api` с `external_controller = 127.0.0.1:<port>` и
    случайным 32-hex `secret`;
  - `TrafficEndpoint::allocate()` берёт свободный loopback-порт
    (`TcpListener::bind(("127.0.0.1", 0))`) и CSPRNG-secret на сессию;
  - `GET /connections` с `Authorization: Bearer`, читаются только
    `uploadTotal`/`downloadTotal`; таймаут 2 с, один HTTP-клиент на процесс,
    лимит тела 1 МиБ;
  - `AppState` хранит endpoint в памяти: новый `connect` — новая сессия,
    `disconnect` — источник сбрасывается, UI показывает нули/«—»;
  - ошибка метрик не ломает подключение: одно предупреждение в лог
    (`Статистика трафика недоступна: …`), `available=false`;
  - команда `traffic_stats` (`#[tauri::command(async)]`, синхронное тело на
    async-рантайме — как `app_icon`/`subscription_fetch`);
  - UI на «Главной»: строка «Трафик за сессию», компонент `SessionTraffic`
    монтируется только при подключении и опрашивает раз в секунду; при ошибке
    «—» без `ErrorBox`; единицы Б/КиБ/МиБ/ГиБ из словарей.
- **Лицензии.** `THIRD_PARTY_NOTICES` в корне; `LICENSE` и notices в бандле.
  Новых зависимостей S4-T4 не добавил.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **274 теста** ok (1 `#[ignore]` —
  реальный скан иконок).
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **66 тестов** ok;
  `pnpm build` ok.
- **Clash API на реальном sing-box 1.14.4** (минимальный конфиг без TUN):
  `sing-box check` → 0; `GET /connections` без secret → `401`; с
  `Authorization: Bearer` → `{"connections":[],"downloadTotal":0,…,"uploadTotal":0}`;
  после 200 000 байт через mixed-inbound — `downloadTotal=200204`,
  `uploadTotal=89`, суммы сохраняются после закрытия соединения.
  Подробности — `docs/research.md` §19.
- **Окно `pnpm tauri dev` (MockHelper):** «Главная» показывает «Трафик за
  сессию: —»; после клика «Подключить» — «Подключено», время сессии идёт,
  трафик остаётся «—», `ErrorBox` нет. System Events в сессии работает
  (клик по accessibility-кнопке).
- **Frontend-тесты:** `formatBytes` (RU/EN, 0/байты/КиБ/МиБ/ГиБ, отрицательные,
  NaN), строка трафика (значения, «—», обновление раз в секунду), совпадение
  ключей `ru`/`en` и отсутствие кириллицы в `en`.
- **Rust-тесты:** парсинг `/connections` (лишние поля игнорируются, отсутствующие
  суммы = 0, битый JSON — ошибка), чтение с Bearer-заголовком, `401`,
  закрытый порт, обрыв тела, случайность порта/secret; `connect` включает
  `experimental.clash_api`, `stop` сбрасывает endpoint; ошибка даёт
  `available=false` и ровно одно предупреждение; golden-конфиги обновлены и
  проходят `sing-box check`.

### Не проверено (причина)

- **Реальные ↑/↓ в окне через helper/TUN** — в сессии нет root и реальной
  подписки. API и формат проверены на реальном ядре, код чтения — тестами.
  Нужно: dev-helper под `sudo`, реальное подключение, трафик выбранного
  приложения; ожидается рост сумм и сброс после переподключения.
- **Статистика после перезапуска окна** — серия живёт в памяти приложения:
  helper может остаться запущенным, но порт/secret потеряны → «—» до
  переподключения. Это задокументировано, не баг.
- **Linux/Windows для S4-T4** — нет машин; Clash API и HTTP-код
  платформонезависимы, но в рантайме не проверялись.
- **Меню трея в EN** — подписи переведены кодом, визуально не проверялось.
- **Установка/запуск LaunchDaemon** — blocked: `security find-identity -v -p
  codesigning` → 0; нужен Apple Development/Developer ID.
- **TUN, per-app, DNS, сон/пробуждение, смена сети на упакованных бинарях** —
  blocked: `sudo` требует пароль, реальной подписки в сессии нет.
- **Linux-рантайм** (S3-T3, S3-T7): пакеты и AppImage собраны, тесты зелёные;
  установка, `systemctl`/`pkexec`, GUI, TUN/per-app/DNS не проверены (нет
  пароля sudo и GUI по SSH).
- **Windows-рантайм** (S3-T4): служба, `sc.exe`, UAC, pipe ACL, TUN,
  `wintun.dll`, NSIS — нет VM.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`
  (`assert.h not found`), Linux — `gio-sys`/`glib-sys`. Платформенные ветки
  проверяйте изолированными крейтами/функциями.
- `sudo -n true` **не проходит**; привилегированные действия — только с явным
  подтверждением пользователя.
- **Debug-сборка** ищет сокет в `$TMPDIR/splitroute-helper.sock`. Установленный
  helper: macOS — `/var/run/splitroute/helper.sock`, Linux —
  `/run/splitroute/helper.sock`, Windows — `\\.\pipe\splitroute-helper`;
  переопределение — `SPLITROUTE_HELPER_SOCKET`/`SPLITROUTE_HELPER_PIPE`.
- **Helper: одно сообщение на соединение.** Smoke-клиент подключается заново.
- **Bundle id** `com.splitroute.app` заканчивается на `.app` — Tauri
  предупреждает; финальный идентификатор решить до публичной сборки (PRD §13.3).
- **Провайдер подписки:** формат зависит от UA; `SplitRoute/0.1.0` → base64
  `vless://` (~1 с), `Happ/4.1.0` → JSON (может идти ~17 с). Сервер нестабилен,
  таймаут 45 с не уменьшать.
- **Linux-машина CachyOS доступна** (доступ выдан 2026-11-14; хост/каталог — в
  чате, в репозиторий не коммитить). Нет git-репозитория: исходники
  синхронизируются `rsync`. `sudo -n` требует пароль; GUI по SSH нет.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`). Секреты не логировать.
- **Clash API (S4-T4):** только `127.0.0.1`, случайный порт и secret на сессию;
  endpoint в `state.json` не сохраняется; в логи/диагностику не попадает.
  `/connections` отдаёт кумулятивные суммы; `/traffic` — поток дельт, не
  используется.
- **UI-автоматизация:** System Events отвечает; клик по accessibility-кнопке
  (`click at {x,y}`) и `key code` работают, координаты считаются из логических
  границ окна (`get {position, size} of window 1`).
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

Выполнять только при появлении сертификата/целевых ОС и с явным подтверждением
пользователя. Привилегированные действия автоматически не запускать.

### macOS: реальные счётчики трафика (S4-T4)

1. `pnpm tauri dev` с реальным helper'ом (не MockHelper), dev-helper под
   `sudo` — см. AGENTS §4.5.
2. Выбрать локацию, подключиться, пустить трафик через выбранное приложение
   (например, `curl` с правилом по процессу или браузер с правилом `.app`).
3. Ожидается: ↑/↓ растут раз в секунду; после «Отключить» — «—»; после
   повторного «Подключить» — снова с нуля. В логах при живом ядре
   предупреждений «Статистика трафика недоступна» нет.

### macOS: LaunchDaemon (S3-T2)

Нужен сертификат Apple Development/Developer ID; ad-hoc не подходит (AMFI:
`has no CMS blob`). Чек-лист — `docs/stage-3-report.md` §6.

### Linux (S3-T3, S3-T7, S2-T9)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
pnpm install
./scripts/fetch-core.sh
./scripts/package.sh             # deb, rpm, appimage
```

Проверить установку/`systemctl`/`pkexec`/TUN/per-app/DNS по матрице
`docs/stage-3-report.md` §9. AppImage без FUSE:
`./SplitRoute_*.AppImage --appimage-extract-and-run`.

### Windows (S3-T4, S4-T2)

`./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh`, затем
`./scripts/portable.sh` или `./scripts/package.sh`; проверить службу под SYSTEM,
UAC, pipe ACL, `wintun.dll`, TUN/per-app, NSIS и лицензию.

---

## Промпт следующей подцели: G4.5 (S4-T5, kill switch)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.5 этапа 4 — S4-T5 «Kill switch (fail-closed)».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3, §6–§7 (среда и подпись), §10 (архитектурные правила), §12–§14,
  §17–§18.
- PRD.md §6.2 (сейчас описано fail-open и «Kill switch (fail-closed) — v0.2»),
  §6.5 (трей), §6.6 (настройки), §6.8 (права helper), §8, §10 (риски).
- TASKS.md: S4-T5 и этап 4; docs/research.md §19 (статистика трафика, текущий
  формат config/connect) и §13 (helper/установщики).
- NEXT_SESSION.md (этот файл), src-tauri/src/{lib.rs,commands/mod.rs,store/mod.rs,
  config/mod.rs,privileged/*}, crates/protocol/src/lib.rs,
  crates/helper/src/{core.rs,handler.rs}, src/features/{connect,settings}/,
  src/shared/{api,state,i18n}/, src-tauri/tests/.

ТЕКУЩЕЕ СОСТОЯНИЕ
- S4-T1…S4-T4, S4-T9, S4-T10 закрыты. Коммит S4-T4 — `cda798e`.
- Подключение: UI → Tauri command `connect` → `build_config` → helper `start`;
  helper запускает sing-box как sidecar и принимает только `status/start/stop`.
  Протокол helper'а версии 1, одно сообщение на соединение; `install`/`uninstall`
  по IPC запрещены.
- `build_config` добавляет `experimental.clash_api` (loopback + secret) для
  статистики; `AppState` хранит endpoint в памяти.
- Конфиг — данные: `serde_json::Value`, `sing-box check` до запуска, TUN
  `auto_route`/`strict_route`, DNS-режимы `proxy`/`system`.
- Проверено 2026-11-14: 274 Rust + 66 frontend тестов, clippy/typecheck/lint/
  build зелёные. Иконки, подписка, локализация и статистика готовы — не ломай.
- `sudo -n true` не проходит: привилегированные проверки возможны только с
  явным участием пользователя. Нет Windows VM; CachyOS доступна по SSH без GUI.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- `crates/protocol` — единственный источник общих типов. **Если kill switch
  требует новой команды helper'а, это осознанное изменение протокола:** подними
  `PROTOCOL_VERSION`, обнови обе стороны, добавь тесты сериализации/диспетчера и
  проверку несовместимости. Без обоснования и правки PRD протокол не меняется.
- Минимальная привилегированная поверхность helper'а: никаких `exec`, shell,
  произвольных путей и правил от клиента. Только фиксированные команды и
  проверенные аргументы.
- UI не получает root, не вызывает shell и не обращается к ядру в обход
  Tauri-команд. Конфиг — данные, не код; локальные API — только `127.0.0.1`.
- Никакой телеметрии. Секреты, URL, HWID и UUID не логировать и не коммитить.
- Новых тяжёлых зависимостей не добавлять без обоснования. Комментарии — на
  русском, идентификаторы — английские.

ЗАДАЧИ
1. Решение и PRD (до кода).
   - Определи, что именно значит fail-closed: при падении ядра/туннеля трафик
     выбранных приложений **не выходит напрямую**. Реши, блокировать только
     выбранные приложения или весь трафик, что происходит при штатном
     `disconnect`, выходе приложения, сне/пробуждении и смене сети.
   - Выбери механизм по ОС и обоснуй минимальный: macOS — `pf` anchor,
     Linux — `nftables` (fallback `iptables`), Windows — WFP/firewall. Отдельно
     опиши, кто ставит и снимает правила (helper при `start`/`stop` или
     отдельная команда), как гарантируется снятие при аварии и как
     пользователь может выключить защиту.
   - Обнови PRD §6.2/§6.6/§6.8/§10 и при необходимости `docs/research.md`
     (раздел с проверенными фактами и ограничениями). Если объём не влезает в
     подцель — разбей S4-T5 на подзадачи (`S4-T5a/b/…`) в TASKS.md, реализуй
     первую проверяемую часть и честно отметь остальное.
2. Реализация.
   - Платформенные правила — в `privileged/`/helper, изолированно по
     `#[cfg(target_os)]`. Никаких shell-строк из UI; аргументы фиксированы.
   - Настройка в UI: включение/выключение kill switch, понятное предупреждение
     о последствиях, состояние (активен/выключен/ошибка). Строки — сразу в
     `ru` и `en`.
   - Если защита активна, при падении ядра UI показывает явную ошибку и не
     делает вид, что трафик идёт через прокси. Не оставляй «залипших» правил
     после `disconnect`/выхода.
3. Тесты.
   - Rust: генерация правил (чистые функции), протокол helper'а при изменении,
     идемпотентность включения/выключения, ошибки прав → типизированный код,
     отсутствие секретов в логах.
   - Frontend: переключатель, состояния ошибки, строки в обоих словарях.
   - `sing-box check` для конфигов не должен сломаться.
4. Ручная проверка macOS (если доступны root и реальный профиль).
   - Подключиться, запустить трафик выбранного приложения, убить ядро
     (`kill <pid>` или через helper) — выбранное приложение не должно выйти
     напрямую; в UI — явное предупреждение. После `disconnect` правила сняты,
     прямой трафик восстановлен. Если root нет — остановись, отметь проверку
     blocked и не выдавай её за выполненную.
5. Документация.
   - TASKS.md: S4-T5 → done (или разбитые подзадачи) со ссылкой на коммит.
   - docs/research.md: проверенные факты, версии, ограничения по ОС.
   - NEXT_SESSION.md: состояние и промпт следующей незакрытой задачи (S4-T6
     автовыбор локации или S4-T7 доменные правила — по статусу TASKS.md).

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Реальный `pnpm tauri dev` (MockHelper) не должен ломаться; при доступных
  root/подписке — ручной сценарий macOS из задачи 4.

ОГРАНИЧЕНИЯ
- Не менять протокол helper'а, установщики, иконки, локализацию и статистику
  сверх задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не выдавать непроверенное на других ОС за проверенное.

ОТЧЁТ
- TASKS.md: статус S4-T5 со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, самодостаточный промпт следующей
  подцели.
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S4-T4 (статистика):** источник — локальный Clash API ядра. Порт и secret
  живут только в памяти; после перезапуска окна статистика покажет «—» до
  переподключения. Считается весь трафик через ядро (proxy + direct через TUN).
  Не добавляйте счётчики в трей и не отправляйте их наружу.
- **S4-T3 (локализация):** новые видимые строки добавляйте в `ru.ts` и
  `en.ts`; `en` — `Record<MessageKey, string>`, тест запрещает кириллицу в
  `en`. Язык меняется только через `settings_update` и применяется сразу.
- **S4-T2 (иконки):** не переносите чтение иконок в `apps_installed` — ответ
  вырастет на сотни килобайт. Ленивая загрузка + кэш по пути — осознанно.
- **S4-T1 (каталог):** macOS-скан намеренно не рекурсивный (контейнеры
  `Utilities` дают неверные имена). «Запущенные» — заглушка, источник
  процессов в backlog.
- **S3-T2 blocked по подписи:** нужен Apple Development/Developer ID.
  Установщик проверяет подпись до копирования.
- **S3-T7 blocked:** Linux/Windows-пункты матрицы не проверены.
- **Bundle id** `com.splitroute.app` даёт предупреждение Tauri; решить до
  публичной сборки (PRD §13.3).
- **Лицензии:** `THIRD_PARTY_NOTICES` обновляется вручную; при смене версии
  sing-box/wintun править разделы 2–3, иначе тест `packaging_config` упадёт —
  это намеренно.
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
- **AppImage 137 МиБ** больше цели PRD 80 МиБ: внутри sing-box 78 МиБ и
  `splitroute` без strip. Оптимизация размера — не блокер.
- **`--allow-uid`/`--allow-sid`** фиксируют одного пользователя (G3.1).
- **Секреты** (URL подписки, HWID, UUID, реальные адреса, `secret` Clash API)
  в задачи, фикстуры, логи, отчёты и коммиты не попадают.
