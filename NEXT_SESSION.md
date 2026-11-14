# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS).
- **Этап 3: S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.**
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked** (нет
  Linux/Windows; macOS-часть выполнена).
- Дата проверки: **2026-11-14**. Ветка `main`; последний коммит — документация
  G3.4 (`git log -1`). Отчёт: [docs/stage-3-report.md](./docs/stage-3-report.md)
  §9. Факты: [docs/research.md](./docs/research.md) §13.
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
  Пароль не читается и не хранится.
- **Лицензии.** `THIRD_PARTY_NOTICES` в корне; `LICENSE` и notices в бандле:
  macOS — `Contents/Resources` (проверено `cmp`), DMG — страница GPL-3.0,
  portable — рядом с бинарём; `.deb`/`.rpm`/AppImage — `/usr/share/doc/splitroute/`,
  NSIS — `licenseFile` (по конфигу и тестам).
- **Упаковка.** `tauri.linux.conf.json`, `tauri.windows.conf.json`
  (`externalBin` только там), `scripts/{fetch-core,fetch-wintun,build-helper-sidecar,package,portable}.sh`.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **183 теста** ok.
- `pnpm typecheck`, `pnpm lint`, `pnpm test` — **35 тестов** ok; `pnpm build` ok.
- `./scripts/package.sh` → `SplitRoute.app` (96.80 МиБ) и `.dmg`
  (32.87 МиБ); sidecar'ы в `Contents/MacOS/`; `LICENSE` и
  `THIRD_PARTY_NOTICES` в `Contents/Resources/` побайтово совпадают с корнем;
  DMG показывает текст GPL-3.0.
- `./scripts/portable.sh` → `SplitRoute-portable-macos-arm64.tar.gz` (33 MiB):
  приложение, `sing-box` 1.14.4, helper, лицензия и notices.
- Запуск `.app` (`open -n`) и корректное завершение по `SIGTERM`; остатков
  `splitroute-helper`/`sing-box run` нет.
- **Smoke упакованными бинарями** (helper и sing-box из `.app`, конфиг без TUN):
  сокет `0600`, `status` → `stopped`, `start` → `running` + pid + версия,
  конфиг `0600`, mixed-inbound слушает, `stop` → `stopped`, конфиг удалён,
  процесс завершён, порт свободен, `SIGTERM` удаляет сокет.
- Helper в бандле: `Signature=adhoc`, `flags=0x20002(adhoc,linker-signed)` —
  установщик отклонит его до копирования (по коду).

### Не проверено (причина)

- **Установка/запуск LaunchDaemon** — blocked: `security find-identity -v -p
  codesigning` → 0; нужен Apple Development/Developer ID. Чек-лист — отчёт §6.
- **TUN, per-app, DNS, сон/пробуждение, смена сети на упакованных бинарях** —
  blocked: `sudo` требует пароль, реальной подписки в сессии нет. Функционально
  проверено 2026-11-12 на dev-бинарях (S2-T8/S2-T10).
- **Linux/CachyOS (S3-T3, S2-T9):** `systemctl`, `pkexec`, права
  `/run/splitroute`, `.deb`/`.rpm`/AppImage — нет машины.
- **Windows (S3-T4):** служба под SYSTEM, `sc.exe`, UAC, pipe ACL, TUN,
  `wintun.dll`, NSIS — нет VM; кросс-сборка установщиков не поддерживается.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`
  (`assert.h not found`), Linux — `gio-sys`/`glib-sys`. Проверяйте
  `splitroute-helper` и платформенные модули изолированно.
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
`--allow-uid` = uid установившего. Для CachyOS дополнительно проверить
`systemd-resolved` и allowlist (S2-T9).

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

---

## Промпт следующей подцели: G4.1 (S4-T1)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.1 этапа 4 — S4-T1 «Каталог установленных приложений (без иконок)».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §2, §6, §8, §10–§14, §17–§18.
- PRD.md §6.3 (каталог установленных приложений), §6.6, §7.
- TASKS.md: S4-T1 и этап 4.
- NEXT_SESSION.md (этот файл), docs/stage-3-report.md §9,
  docs/research.md §13.
- src-tauri/src/commands/mod.rs, src/shared/api/{client,types}.ts,
  src/features/apps/, crates/protocol/src/model.rs.

ТЕКУЩЕЕ СОСТОЯНИЕ
- Этап 3 закрыт по коду: helper, установщики macOS/Linux/Windows, UI установки,
  лицензии, упаковка. S3-T2 blocked подписью Apple, S3-T7 blocked отсутствием
  Linux/Windows — их не «закрывать» и не выдавать за проверенные.
- Проверено 2026-11-14: 183 Rust + 35 frontend тестов, clippy -D warnings,
  сборка .app/.dmg/portable, smoke упакованных helper + sing-box без root.
- Каталога установленных приложений нет: экран «Приложения» умеет только
  правила (rules_list/rules_upsert/rules_delete) и не перечисляет установленные
  приложения.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник общих типов; формат IPC helper'а
  (status/start/stop) не меняется.
- InstallerError::{RequiresPrivileges,Unsupported,MissingBinary,Failed} и коды
  CommandError INSTALLER_* — контракт UI.
- UI не получает root, не вызывает shell и не сканирует систему в обход
  Tauri-команд. Никаких привилегированных действий в этой задаче.
- AppRule (id, name, platform, process_name, process_path, path_regex, enabled)
  не меняется: каталог только создаёт правила из него.
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- externalBin — только в платформенных конфигах; LICENSE и
  THIRD_PARTY_NOTICES остаются в бандле.
- Комментарии — на русском, идентификаторы — английские; новых зависимостей
  не добавлять без обоснования.

ЗАДАЧИ
1. Модель и команда.
   - Добавить в crates/protocol тип InstalledApp { name: String, path: String,
     platform: Platform } (serde, PartialEq, Eq) и тест round-trip.
   - Добавить Tauri-команду `apps_installed() -> Vec<InstalledApp>`.
     Источники:
     macOS — каталоги *.app в /Applications, /System/Applications,
     ~/Applications (один уровень, без рекурсии); имя — stem бандла, path —
     путь к .app;
     Windows — ярлыки Start Menu (ProgramData и пользователя) и/или записи
     реестра Uninstall с DisplayIcon; Linux — *.desktop в
     /usr/share/applications и ~/.local/share/applications, путь — из Exec
     (без аргументов), имя — из Name.
   - Разбор каждого источника — чистая функция, тестируемая на macOS
     (фикстуры строк/временные каталоги), без обращения к реальной системе
     в тестах.
   - Не падать на битых ярлыках/desktop-файлах: пропускать с причиной в логе.
   - Дедупликация по path, сортировка по имени.
2. UI.
   - Экран «Приложения»: вкладка «Установленные» (по PRD §7: Запущенные /
     Установленные / Все правила), поиск по имени.
   - Действие «Добавить правило» создаёт выключенное правило через
     rules_upsert: macOS — path_regex `^<экранированный путь .app>/`
     (экранировать символы regex), Windows/Linux — process_path с полным
     путём; имя правила = имя приложения; platform = текущая ОС.
   - Ошибки и пустое состояние — по-русски, через существующий i18n.
   - Ничего не подключать и не менять на других экранах.
3. Тесты.
   - Rust: round-trip InstalledApp, парсинг macOS/Windows/Linux источников на
     фикстурах, дедупликация, экранирование regex, пропуск битых записей.
   - Frontend: вкладка «Установленные», поиск, создание правила из записи,
     пустое состояние и ошибка команды.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build

ОГРАНИЧЕНИЯ
- Не менять протокол helper'а, установщики и экраны сверх описанного.
- Не выполнять привилегированные действия и не сканировать чужие каталоги.
- Не выдавать непроверенное на других ОС за проверенное.

ОТЧЁТ
- TASKS.md: S4-T1 → done со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, и самодостаточный промпт G4.2
  (S4-T2, иконки приложений).
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
- В `/tmp` остался `sr-meta.json` (вывод `cargo metadata` прошлой сессии,
  ~2.6 МБ) — к репозиторию не относится, можно удалить.
- Известные ограничения G3.1: osascript-скрипт во временном каталоге (TOCTOU —
  для релиза SMJobBless/Authorization Services или подписанный установщик);
  `--allow-uid`/`--allow-sid` фиксируют одного пользователя.
- Подпись нужна для распространения: Developer ID + notarization (macOS),
  Authenticode/SmartScreen (Windows), подпись пакетов (Linux); `wintun.dll` —
  только официальная подписанная.
