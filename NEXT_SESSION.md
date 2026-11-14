# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS).
- **Этап 3: S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.**
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked** (нет
  Linux/Windows; macOS-часть выполнена).
- **Этап 4: S4-T1 — done.** Следующая — S4-T2 (иконки приложений).
- Дата проверки: **2026-11-14**. Ветка `main`; последний коммит — документация
  G4.1 (`git log -1`). Отчёт этапа 3: [docs/stage-3-report.md](./docs/stage-3-report.md)
  §9. Факты: [docs/research.md](./docs/research.md) §13–§14.
- Системные сервисы, сокеты, пайпы и процессы после проверок не остались.

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
- **Лицензии.** `THIRD_PARTY_NOTICES` в корне; `LICENSE` и notices в бандле
  (macOS — `Contents/Resources`, `.deb`/`.rpm`/AppImage —
  `/usr/share/doc/splitroute/`, NSIS — `licenseFile`).
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **208 тестов** ok.
- `pnpm typecheck`, `pnpm lint`, `pnpm test` — **41 тест** ok; `pnpm build` ok.
- Реальный скан macOS: **110 приложений** из трёх каталогов, сортировка и
  дедупликация корректны; контейнеры (`/System/Applications/Utilities`,
  `Chrome Apps.localized`) пропущены как не-`.app`.
- Windows-ветка реестра (`collect`/`uninstall_roots`/`read_uninstall_key`)
  типизирована отдельной `cargo check --target x86_64-pc-windows-msvc` с
  `windows-sys 0.61` — чисто. Полный `cargo check -p splitroute` под MSVC
  по-прежнему падает на `ring` (`assert.h not found`).
- Ранее (этап 3): сборка `.app`/`.dmg`/portable, smoke упакованных helper +
  sing-box без root — см. `docs/stage-3-report.md` §9.

### Не проверено (причина)

- **Установка/запуск LaunchDaemon** — blocked: `security find-identity -v -p
  codesigning` → 0; нужен Apple Development/Developer ID. Чек-лист — отчёт §6.
- **TUN, per-app, DNS, сон/пробуждение, смена сети на упакованных бинарях** —
  blocked: `sudo` требует пароль, реальной подписки в сессии нет.
- **Linux/CachyOS (S3-T3, S2-T9):** `systemctl`, `pkexec`, права
  `/run/splitroute`, `.deb`/`.rpm`/AppImage — нет машины. Каталог `.desktop`
  на реальном Linux тоже не проверен.
- **Windows (S3-T4):** служба под SYSTEM, `sc.exe`, UAC, pipe ACL, TUN,
  `wintun.dll`, NSIS — нет VM. Реальные ярлыки и реестр `Uninstall` не
  проверены, только фикстуры.
- **Пометка несуществующих путей у сохранённых правил** (PRD §6.3) — не
  реализована; вне объёма S4-T1.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`
  (`assert.h not found`), Linux — `gio-sys`/`glib-sys`. Для Windows-веток
  каталога проверяйте функции в изолированном крейте с `windows-sys 0.61`.
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
  неверные имена. Иконки — отдельная задача S4-T2.
- После проверок: `pgrep -fl 'splitroute-helper|sing-box run'`,
  `ls -d /tmp/sr-*`, `launchctl print system/com.splitroute.helper`.

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

### Linux (S3-T3, S2-T9)

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
`--allow-uid` = uid установившего. Дополнительно для S4-T1: вкладка
«Установленные» показывает приложения из `/usr/share/applications` и
`~/.local/share/applications`, поиск фильтрует, битые `.desktop` попадают в
логи (`Debug`) и не ломают список.

### Windows (S3-T4)

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
Для S4-T1: каталог находит ярлыки «Пуск» и записи `Uninstall`, правило
создаётся с `process_path` к `.exe`.

---

## Промпт следующей подцели: G4.2 (S4-T2)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.2 этапа 4 — S4-T2 «Иконки приложений в каталоге».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §2, §6, §8, §10–§14, §17–§18.
- PRD.md §6.3, §7 (экран «Приложения»).
- TASKS.md: S4-T2 и этап 4.
- NEXT_SESSION.md (этот файл), docs/research.md §14,
  docs/stage-3-report.md §9.
- src-tauri/src/catalog/{mod,macos,linux,windows}.rs,
  src-tauri/src/commands/mod.rs, src/shared/api/{client,types}.ts,
  src/features/apps/, crates/protocol/src/model.rs.

ТЕКУЩЕЕ СОСТОЯНИЕ
- S4-T1 done (коммит f4e4eab): тип InstalledApp {name, path, platform},
  команда apps_installed (macOS — .app, Linux — .desktop, Windows — ярлыки
  и реестр), app_rule_from_installed, вкладка «Установленные» с поиском.
- Проверено 2026-11-14: 208 Rust + 41 frontend тестов, clippy -D warnings,
  реальный скан macOS (110 приложений). Linux/Windows-скан не проверен.
- Иконок нет: записи каталога отдаются без изображений.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник общих типов; формат IPC helper'а
  (status/start/stop) не меняется.
- AppRule и InstalledApp не получают необязательных полей «на будущее» без
  согласования: сначала правка PRD/типа, затем код.
- UI не получает root, не вызывает shell и не сканирует систему в обход
  Tauri-команд. Никаких привилегированных действий.
- Иконки не должны блокировать UI и не должны читаться на каждый рендер.
- Секреты не логировать и не коммитить. Новых тяжёлых зависимостей не
  добавлять без обоснования (AGENTS.md §3).
- Комментарии — на русском, идентификаторы — английские.

ЗАДАЧИ
1. Модель и команда.
   - Расширить InstalledApp полем иконки (например `icon_png_base64:
     Option<String>`) ИЛИ отдельной командой `app_icon(path) -> Option<String>`
     — выбери вариант с меньшим объёмом данных при старте и обоснуй в
     комментарии. Обнови round-trip тест.
   - Источники: macOS — `.icns` из `Contents/Resources` (конвертация без
     тяжёлых зависимостей; допустимо вернуть исходный формат, если UI умеет),
     Linux — `Icon=` из `.desktop` с поиском в hicolor-темах,
     Windows — иконка `.exe`/ярлыка (Shell API или встроенный ресурс).
     Если для ОС библиотека недоступна — честный fallback, не паника.
   - Ограничить размер: иконка ужимается до разумного (например 64×64),
     кэш в памяти по пути, повторный запрос не читает диск.
2. UI.
   - Вкладка «Установленные» показывает иконку слева от имени; при
     отсутствии — заглушка с первой буквой/иконкой приложения.
   - Каталог из 100+ записей не должен заметно тормозить: проверь на реальном
     macOS-скане, при необходимости — ленивая загрузка иконок.
3. Тесты.
   - Rust: разбор/выбор иконки на фикстурах, fallback, кэш (повторный вызов
     не читает диск), лимит размера.
   - Frontend: иконка отображается, fallback при её отсутствии, каталог
     рендерится без ошибок при пустых иконках.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Реальный скан macOS: время открытия вкладки и визуально корректные иконки.

ОГРАНИЧЕНИЯ
- Не менять протокол helper'а, установщики и экраны сверх описанного.
- Не выполнять привилегированные действия и не сканировать чужие каталоги.
- Не выдавать непроверенное на других ОС за проверенное.

ОТЧЁТ
- TASKS.md: S4-T2 → done со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, и самодостаточный промпт G4.3
  (S4-T3, EN-локализация).
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
  упадёт — это намеренно.
- **Упаковка:** `externalBin` только в платформенных конфигах
  (`packaging_config.rs`); перед `tauri build` — `fetch-core.sh` и
  `build-helper-sidecar.sh`.
- **Smoke helper'а:** одно сообщение на соединение; клиент приложения
  переподключается на каждую команду.
- **Каталог приложений:** macOS-скан намеренно не рекурсивный (контейнеры
  `Utilities` дали бы неверные имена). `Exec`/`DisplayIcon` нормализуются
  чистыми функциями — их и расширяйте тестами, а не системными вызовами.
- **`--allow-uid`/`--allow-sid`** фиксируют одного пользователя (G3.1).
- Подпись нужна для распространения: Developer ID + notarization (macOS),
  Authenticode/SmartScreen (Windows), подпись пакетов (Linux); `wintun.dll` —
  только официальная подписанная.
