# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS).
- **Этап 3: S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.**
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked** (нет
  Linux/Windows; macOS-часть выполнена).
- **Этап 4: S4-T1 и S4-T2 — done.** Следующая — S4-T3 (EN-локализация).
- Дата проверки: **2026-11-14**. Ветка `main`. Последние коммиты: код G4.2 —
  `ae1fe6d`, документация G4.2 — следующий (`git log -1`). Отчёт этапа 3:
  [docs/stage-3-report.md](./docs/stage-3-report.md) §9. Факты:
  [docs/research.md](./docs/research.md) §13–§15.
- Системные сервисы, сокеты, пайпы, dev-серверы и процессы после проверок не
  остались.

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
  named pipe ACL, `icacls`, UAC). macOS/Linux после установки ждут сокет.
- **UI установки.** Коды `INSTALLER_PRIVILEGES`, `INSTALLER_UNSUPPORTED`,
  `INSTALLER_MISSING_BINARY`, `INSTALLER_FAILED`; `ErrorBox` показывает
  локализованный заголовок и точный текст установщика. Состояния helper'а:
  установлен / не установлен / устарел (`HELPER_PROTOCOL`) / не отвечает
  (`HELPER_TIMEOUT`/`HELPER_UNAVAILABLE`) — `src/shared/lib/helperState.ts`.
- **Каталог приложений (S4-T1).** `crates/protocol::InstalledApp {name, path,
  platform}`; команда `apps_installed` (`src-tauri/src/catalog/`):
  - macOS — `.app` в `/Applications`, `/System/Applications`, `~/Applications`,
    один уровень без рекурсии; имя — stem бандла;
  - Linux — `.desktop` в `/usr/share/applications` и
    `~/.local/share/applications`; имя из `Name`, путь из первого токена `Exec`
    (снимается `env VAR=…`, отбрасываются field-коды); относительные и
    отсутствующие бинари пропускаются;
  - Windows — ярлыки меню «Пуск» (ProgramData + APPDATA, рекурсивно) и
    `Uninstall` в HKLM/HKCU/WOW6432Node по `DisplayIcon`; свой разбор Shell Link.
  - Битые записи пропускаются с причиной в лог `Debug`, результат
    дедуплицируется по `path` и сортируется по имени.
  - `app_rule_from_installed` создаёт **выключенное** правило: macOS —
    `path_regex ^<escaped .app>/`, Windows/Linux — `process_path`.
  - UI: вкладки «Запущенные / Установленные / Все правила» (PRD §7), поиск по
    имени, пустые состояния, типизированная ошибка с повтором.
- **Иконки каталога (S4-T2).** `InstalledApp` не менялся; отдельная команда
  `app_icon(app)` (`#[tauri::command(async)]`) отдаёт готовый `data:` URL или
  `null`:
  - macOS — PNG-чанк из `.icns` в `Contents/Resources` (минимальная ширина
    ≥ 64; `Info.plist` не парсится);
  - Linux — `Icon=` из `.desktop`, поиск в `hicolor` по размерам 64/48/128/
    256/32/512, затем `scalable` (SVG), затем `/usr/share/pixmaps`;
  - Windows — `PrivateExtractIconsW` 64×64 (fallback `ExtractIconExW`),
    `GetDIBits` BGRA, прозрачность из AND-маски.
  - Нормализация до 64 px (`image`, png-only), лимит 256 КиБ, свой base64;
    кэш в памяти по пути, негативный результат тоже; карта `exec → Icon` на
    Linux строится один раз.
  - UI: `AppIcon` 32×32, ленивая загрузка по `IntersectionObserver`
    (`rootMargin: 200px`), модульный кэш по `app.path` (включая «иконки нет»),
    заглушка с первой буквой; ошибка иконки не показывает `ErrorBox`.
- **Лицензии.** `THIRD_PARTY_NOTICES` в корне; `LICENSE` и notices в бандле
  (macOS — `Contents/Resources`, `.deb`/`.rpm`/AppImage —
  `/usr/share/doc/splitroute/`, NSIS — `licenseFile`). Новых пакетов S4-T2 не
  добавил: `image` уже был транзитивно через `tauri/image-png`.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **237 тестов** ok (1 `#[ignore]` —
  реальный скан иконок).
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **51 тест** ok;
  `pnpm build` ok.
- Реальный скан macOS: **110 приложений**, **100 иконок**. `cargo test -p
  splitroute --lib catalog::icons::tests::real_catalog_icons_are_found --
  --ignored`: скан 0.4 мс, холодная загрузка всех иконок 1.35 с (debug), из
  кэша 0.12 мс; суммарно 659 КиБ, максимум 12 КиБ на иконку.
- Реальное окно `pnpm tauri dev` (`SPLITROUTE_MOCK_HELPER=1`): вкладка
  «Установленные» показывает иконки Adguard, Antigravity, AnyDesk, App Store
  32×32 и заглушки без ошибок. Автоматизировать клики не удалось: System
  Events отвечает `-1728` (нет разрешения Accessibility) — проверка визуальная
  по скриншоту, вкладку временно открывали сменой `initialState.activeTab`.
- Windows-код иконок (`windows_icons.rs`) типизирован изолированно:
  `cargo check --all-targets --target x86_64-pc-windows-msvc` с
  `windows-sys 0.61` — ок. Полный `cargo check -p splitroute` под MSVC
  по-прежнему падает на `ring` (`assert.h not found`).
- Windows-ветка реестра (`collect`/`uninstall_roots`/`read_uninstall_key`)
  типизирована отдельной проверкой (S4-T1).

### Не проверено (причина)

- **Иконки Linux в рантайме** — нет Linux-машины: `.desktop` `Icon=`,
  hicolor-PNG/SVG и приоритет размеров проверены синтетическими каталогами.
  SVG отдаётся как `data:image/svg+xml` без растеризации, XPM не поддержан.
- **Иконки Windows в рантайме** — нет Windows VM: проверены компиляция
  Win32-вызовов и чистые функции BGRA/маски. Поведение `GetDIBits` на реальных
  `.exe` (и допущение «белый пиксель маски = прозрачный») не подтверждено.
- **10 из 110 приложений macOS без иконки** — предел источника: у 7
  wrapper-бандлов нет своего `.icns`, у Calendar/Photo Booth/System Settings
  иконка только в `Assets.car`. В UI — заглушка, не ошибка.
- **Установка/запуск LaunchDaemon** — blocked: `security find-identity -v -p
  codesigning` → 0; нужен Apple Development/Developer ID. Чек-лист — отчёт §6.
- **TUN, per-app, DNS, сон/пробуждение, смена сети на упакованных бинарях** —
  blocked: `sudo` требует пароль, реальной подписки в сессии нет.
- **Linux/CachyOS (S3-T3, S2-T9):** `systemctl`, `pkexec`, права
  `/run/splitroute`, `.deb`/`.rpm`/AppImage — нет машины. Каталог `.desktop`
  и иконки на реальном Linux тоже не проверены.
- **Windows (S3-T4):** служба под SYSTEM, `sc.exe`, UAC, pipe ACL, TUN,
  `wintun.dll`, NSIS — нет VM. Реальные ярлыки, реестр `Uninstall` и иконки
  `.exe` не проверены, только фикстуры и компиляция.
- **Пометка несуществующих путей у сохранённых правил** (PRD §6.3) — не
  реализована; вне объёма S4-T1/S4-T2.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`
  (`assert.h not found`), Linux — `gio-sys`/`glib-sys`. Для Windows-веток
  каталога/иконок проверяйте функции изолированным крейтом с `windows-sys 0.61`
  и `--target x86_64-pc-windows-msvc`.
- `sudo -n true` **не проходит**; системные службы не ставить без явного
  действия пользователя.
- **Debug-сборка** ищет сокет в `$TMPDIR/splitroute-helper.sock`. Для
  установленного helper'а: macOS — `/var/run/splitroute/helper.sock`
  (`SPLITROUTE_HELPER_SOCKET`), Linux — `/run/splitroute/helper.sock`,
  Windows — `\\.\pipe\splitroute-helper` (`SPLITROUTE_HELPER_PIPE`).
- **`externalBin` только в платформенных конфигах**; инвариант закреплён
  `src-tauri/tests/packaging_config.rs`. Перед `tauri build` нужны
  `./scripts/fetch-core.sh` (и `fetch-wintun.sh` на Windows).
- **Helper: одно сообщение на соединение.** Smoke-клиент должен подключаться
  заново на каждую команду.
- **Bundle id `com.splitroute.app` заканчивается на `.app`:** Tauri
  предупреждает о конфликте с расширением бандла. До публичной сборки
  зафиксировать финальный идентификатор (PRD §13.3); смена затронет каталог
  состояния и имена служб.
- Установщик ищет helper и sing-box: `SPLITROUTE_HELPER_BIN` /
  `SPLITROUTE_SING_BOX_BIN`, рядом с приложением, либо в
  `src-tauri/binaries/sing-box-<triple>` (dev).
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- **`THIRD_PARTY_NOTICES` обновляется вручную** (раздел 11 файла):
  `cargo metadata --all-features`, `pnpm licenses list --prod`.
- **Каталог приложений:** пути читаются из системных списков; привилегий нет.
  Не добавляйте рекурсию в macOS-скан молча — контейнеры (`Utilities`) дадут
  неверные имена.
- **Иконки:** кэш в Rust и в UI живёт до перезапуска процесса; «Обновить
  список» его не сбрасывает (осознанно — иконка приложения меняется только с
  обновлением самой программы). `.icns` без PNG-чанков, wrapper-бандлы и
  `Assets.car` дают `null` → заглушка, это ожидаемо.
- **UI-тексты** сейчас только `ru` в `src/shared/i18n/ru.ts`; `errorText`
  берёт локализованный заголовок по коду, а `message` от Rust показывается
  как есть (русский).
- После проверок: `pgrep -fl 'splitroute-helper|sing-box run'`,
  `pgrep -fl 'target/debug/splitroute'`, `ls -d /tmp/sr-*`,
  `launchctl print system/com.splitroute.helper`, `lsof -nP -iTCP:1420`.

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
pgrep -fl 'splitroute-helper|sing-box run' || echo "процессов нет"
```

---

## Ручные проверки, которые остаются blocked

Выполнять только при появлении сертификата/целевых ОС и с явным подтверждением
пользователя. Привилегированные действия автоматически не запускать.

### macOS: LaunchDaemon (S3-T2)

Нужен сертификат Apple Development/Developer ID; ad-hoc не подходит (AMFI:
`has no CMS blob`). Подписать helper, затем пройти чек-лист
`docs/stage-3-report.md` §6: `codesign -dv` (не `adhoc`), установка через UI,
`launchctl print system/com.splitroute.helper`, права `/var/run/splitroute`
(`drwx--x--x root wheel`) и сокета (`srw------- <пользователь>:wheel`),
подключение/отключение, удаление, отсутствие процессов.

### Linux (S3-T3, S2-T9, S4-T2)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
./scripts/fetch-core.sh
./scripts/package.sh
dpkg-deb -c target/release/bundle/deb/*.deb | grep share/doc
sudo apt install ./target/release/bundle/deb/*.deb
# UI → Настройки → Helper → «Установить» (pkexec)
systemctl status splitroute-helper.service
ls -ld /run/splitroute
stat -c '%a %U:%G %n' /run/splitroute/helper.sock
pgrep -a -f 'splitroute-helper|sing-box run'
```

Ожидается: `/usr/share/doc/splitroute/{LICENSE,THIRD_PARTY_NOTICES}`;
сервис `active (running)`; каталог `711 root:root`; сокет `600` пользователя;
`--allow-uid` = uid установившего. Для S4-T1/S4-T2: вкладка «Установленные»
показывает приложения из `/usr/share/applications` и
`~/.local/share/applications`, иконки из `hicolor` (PNG и SVG) отрисованы,
поиск фильтрует, битые `.desktop` попадают в логи (`Debug`) и не ломают
список; приложение без `Icon=` показывает заглушку. Отдельно проверить
Flatpak/Snap-приложения: путь из `Exec` может быть sandbox-путём.

### Windows (S3-T4, S4-T2)

На Windows 10/11 x64 с VS Build Tools, Rust MSVC, Node 22 + pnpm, WebView2 и
Git Bash:

```bash
cd app
rustup target add x86_64-pc-windows-msvc
./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh
./scripts/portable.sh          # распаковать и запустить start.bat (UAC)
./scripts/package.sh           # NSIS-установщик
```

Проверить: `sc.exe query splitroute-helper-service` → `RUNNING` под SYSTEM;
`wintun.dll` рядом с `sing-box.exe`; pipe доступен только SID пользователя;
NSIS показывает страницу лицензии; `LICENSE` и `THIRD_PARTY_NOTICES` в
каталоге приложения; TUN поднимается, выбранное приложение идёт через прокси.
Для S4-T1/S4-T2: каталог находит ярлыки «Пуск» и записи `Uninstall`, правило
создаётся с `process_path` к `.exe`; иконки `.exe` отрисованы с прозрачным
фоном (проверить старые `.exe` с 24bpp-ресурсом — там альфа берётся из маски).

---

## Промпт следующей подцели: G4.3 (S4-T3, EN-локализация)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.3 этапа 4 — S4-T3 «EN-локализация».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3, §10 (п. 8 — единый источник состояния в Rust), §12–§14, §17–§18.
- PRD.md §6.6 (настройки), §7 (экраны), §8 (локализация).
- TASKS.md: S4-T3 и этап 4.
- NEXT_SESSION.md (этот файл), docs/research.md §15.
- src/shared/i18n/ru.ts, src/app/, src/features/*, src/shared/ui/,
  src/shared/api/{client,types}.ts, src/shared/lib/helperState.ts,
  src-tauri/src/store.rs, src-tauri/src/commands/mod.rs (CommandError),
  src-tauri/src/privileged/installer*.rs.

ТЕКУЩЕЕ СОСТОЯНИЕ
- Все видимые строки UI уже вынесены в `src/shared/i18n/ru.ts`; `t(key, vars)`
  подставляет `{vars}`, `errorText(code)` даёт локализованный заголовок по
  стабильному коду. Словарь — один (`ru`), `MessageKey = keyof typeof ru`.
- Ошибки Rust приходят как `CommandError { code, message }`; `ErrorBox`
  показывает локализованный заголовок по коду и `message` установщика как
  есть (сейчас по-русски). Коды ошибок стабильны и не переводятся.
- Проверено 2026-11-14: 237 Rust + 51 frontend тестов, clippy -D warnings,
  typecheck/lint/build зелёные. Иконки каталога (S4-T2) готовы, их не трогай.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- `crates/protocol` — единственный источник общих типов; формат IPC helper'а
  (status/start/stop) не меняется.
- Новые поля моделей — только после правки PRD/типа в том же изменении.
- UI не получает root, не вызывает shell и не сканирует систему в обход
  Tauri-команд.
- Секреты не логировать и не коммитить. Новых тяжёлых зависимостей не
  добавлять (AGENTS.md §3): i18n-фреймворк не нужен, словари — свои объекты.
- Комментарии — на русском, идентификаторы — английские. Все видимые строки —
  только через словари, без захардкоженного текста в компонентах.

ЗАДАЧИ
1. Словари и типы.
   - `ru` остаётся эталоном ключей; добавить `en: Record<MessageKey, string>`.
   - `t(key, vars)` выбирает словарь по текущему языку; `errorText(code)` —
     тоже. Ключ, отсутствующий в `en`, не должен молча падать: тест
     равенства наборов ключей обязателен.
   - Язык: `ru` и `en`; значения — `"ru" | "en"`, serde-friendly.
2. Хранение языка (решение зафиксировать в PRD до кода).
   - Предпочтительно — единый источник состояния в Rust (AGENTS §10.8):
     `AppSettings.preferences.language` с `#[serde(default)]` = `ru`, чтобы
     старые `state.json` читались. UI переключает язык в настройках, применяет
     без перезапуска, значение сохраняется через `settings_update`.
   - Проверить, что смена языка не сбрасывает остальные настройки и не
     перезагружает окно; миграция старого state без поля — тест serde.
   - Если выберешь localStorage вместо Rust — обосновать в PRD/отчёте, но
     правило «единое состояние в Rust» тогда нарушается; по умолчанию — Rust.
3. UI.
   - Переключатель языка в «Настройках» (русский/английский), подпись и
     подсказка — через словари.
   - Все экраны (Главная, Локации, Приложения, Настройки, Логи), пустые
     состояния, ошибки, тултипы/`aria-label`, кнопки трея — без русских
     литералов в компонентах. Трей (`src-tauri/src/tray.rs`) — отдельно:
     решить, нужен ли перевод пунктов меню; если да, Rust должен отдавать
     строки по выбранному языку, если нет — зафиксировать в PRD/отчёте.
   - `message` от Rust (установщик, helper) остаётся как есть, если не
     добавляешь маппинг кодов; тогда в EN-режиме он показывается как деталь,
     а заголовок — локализованный. Это нужно явно описать в отчёте.
4. Тесты.
   - Frontend: наборы ключей `ru`/`en` совпадают; переключение языка меняет
     видимый текст (например заголовок экрана и подпись кнопки); отсутствие
     непереведённых ключей; `errorText` для EN; сохранение выбора через
     `settingsUpdate`.
   - Rust: serde `language` (старый state без поля → `ru`, round-trip,
     неизвестное значение → ошибка или default — решить и покрыть тестом).
5. Документация.
   - PRD §6.6/§8: язык интерфейса, где хранится, что EN покрывает (UI;
     сообщения ядра — по решению).
   - docs/research.md: новый раздел с проверенными фактами.
   - TASKS.md: S4-T3 → done со ссылкой на коммит; NEXT_SESSION.md — состояние
     и промпт G4.4 (S4-T4, статистика трафика).

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Реальный запуск `pnpm tauri dev` (MockHelper): переключить язык, пройти все
  пять экранов, убедиться, что русского текста не осталось (кроме
  backend-деталей по решению), перезапустить приложение — выбор сохранился.
  Автоматизация UI недоступна (System Events `-1728`), поэтому проверка
  визуальная по скриншотам; вкладку можно временно открыть сменой
  `initialState.activeTab`, затем обязательно вернуть и проверить `git diff`.

ОГРАНИЧЕНИЯ
- Не менять протокол helper'а, установщики, иконки и экраны сверх локализации.
- Не выполнять привилегированные действия.
- Не выдавать непроверенное на других ОС за проверенное.

ОТЧЁТ
- TASKS.md: S4-T3 → done со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, и самодостаточный промпт G4.4
  (S4-T4, статистика трафика сессии).
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S3-T2 blocked по подписи:** нужен Apple Development/Developer ID. Установщик
  проверяет подпись до копирования; helper в бандле ad-hoc/linker-signed.
- **S3-T7 blocked:** Linux/Windows-пункты не проверены. macOS-часть — отчёт §9.
- **TUN на упакованных бинарях** не проверен: нет root и подписки. Не регресс —
  dev-бинари проверены в S2-T8/S2-T10.
- **Bundle id** `com.splitroute.app` даёт предупреждение Tauri (суффикс `.app`).
  Решить до публичной сборки (PRD §13.3).
- **Лицензии:** `THIRD_PARTY_NOTICES` обновляется вручную; при смене версии
  sing-box/wintun править разделы 2–3 и ссылки, иначе `packaging_config`-тест
  упадёт — это намеренно. `image` уже был в списке транзитивных зависимостей,
  поэтому S4-T2 notices не менял.
- **Упаковка:** `externalBin` только в платформенных конфигах
  (`packaging_config.rs`); перед `tauri build` — `fetch-core.sh` и
  `build-helper-sidecar.sh`.
- **Smoke helper'а:** одно сообщение на соединение; клиент приложения
  переподключается на каждую команду.
- **Каталог приложений:** macOS-скан намеренно не рекурсивный (контейнеры
  `Utilities` дали бы неверные имена). `Exec`/`DisplayIcon` нормализуются
  чистыми функциями — их и расширяйте тестами, а не системными вызовами.
- **Иконки:** не переносить чтение иконок в `apps_installed` — ответ вырастет
  на сотни килобайт. Ленивая загрузка + кэш по пути — осознанное решение.
  Не добавляйте `Assets.car`/wrapper-бандлы молча: это отдельный парсер
  compiled asset catalog; сейчас такие приложения дают заглушку.
- **`--allow-uid`/`--allow-sid`** фиксируют одного пользователя (G3.1).
- Подпись нужна для распространения: Developer ID + notarization (macOS),
  Authenticode/SmartScreen (Windows), подпись пакетов (Linux); `wintun.dll` —
  только официальная подписанная.
