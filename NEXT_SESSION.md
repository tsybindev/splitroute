# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 1–2 закрыты. **Этап 3, подцели G3.1 и G3.2 выполнены по коду.**
- **S3-T1 (Rust-helper) — done.** Коммит `d7af3ba`.
- **S3-T2 (macOS LaunchDaemon) — blocked по подписи.** Код готов (`d9da641`),
  реальная установка проверена до границы AMFI: ad-hoc/linker-подпись helper'а
  убивается (`has no CMS blob`), сертификата на машине нет.
- **S3-T3 (Linux: systemd + polkit) — done по коду.** `be1d918`, `684e6e5`,
  `276f218`. Ручная проверка — blocked (нет Linux-машины).
- **S3-T4 (Windows: служба под SYSTEM) — done по коду.** `be1d918`,
  `684e6e5`, `276f218`, `cf26d1a`. Ручная проверка — blocked (нет Windows VM).
- Ветка: `main`. Отчёт: [docs/stage-3-report.md](./docs/stage-3-report.md).
  Факты: [docs/research.md §10–11](./docs/research.md).
- Системные сервисы, сокеты, пайпы и процессы после проверок не остались.

### Что работает

- **Helper, оба транспорта.** `crates/helper`: Unix-сокет `0600` (uid пира
  `getpeereid`/`SO_PEERCRED`) и Windows named pipe
  `\\.\pipe\splitroute-helper` с SDDL-ACL только на SID пользователя, SYSTEM и
  Administrators плюс сверка SID процесса-клиента. Общие `handler`/`ipc`/
  `platform`; протокол не менялся: `status`/`start`/`stop`,
  `install`/`uninstall` по IPC запрещены.
- **Служба Windows.** `splitroute-helper-service` под SYSTEM:
  `StartServiceCtrlDispatcherW`, `SERVICE_CONTROL_STOP` → остановка ядра и
  выход, статусы start pending/running/stopped. Ожидание клиента вынесено в
  отдельный поток, поэтому STOP не подвисает на `ConnectNamedPipe`.
- **Установщик Linux.** `LinuxInstaller`: юнит `splitroute-helper.service`
  (`User=root`, `RuntimeDirectory=splitroute`, `0711`, `UMask=0077`,
  `Restart=on-failure`), `/usr/lib/splitroute/{splitroute-helper,sing-box}`,
  сокет `/run/splitroute/helper.sock`, установка/удаление через
  `pkexec /bin/sh`; отмена = код 126, нет прав = 127.
- **Установщик Windows.** `WindowsInstaller`: `C:\Program Files\SplitRoute`
  (`wintun.dll` рядом с `sing-box.exe`), runtime `C:\ProgramData\SplitRoute`
  с `icacls`, `sc.exe create ... obj= LocalSystem` (`binPath` одним
  экранированным токеном), UAC через `Start-Process -Verb RunAs`, отмена = 1223.
- **Клиент.** Протокол вынесен из транспорта: `UnixHelperClient` и
  `WindowsPipeClient`. На Windows приложение больше не падает в mock.
- **Упаковка.** `tauri.linux.conf.json` (deb/rpm/appimage),
  `tauri.windows.conf.json` (NSIS perMachine), `externalBin` только там;
  `scripts/build-helper-sidecar.sh`, `scripts/package.sh`.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-12)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто.
- `cargo test --workspace` — 173 теста: `splitroute` 108 (macOS 12 + Linux 13 +
  Windows 15 установщиков) + golden 4 + фикстура 1 + round-trip 1 + упаковка 6,
  `splitroute-helper` 26 + 11 интеграционных, `splitroute-protocol` 16.
- `pnpm typecheck && pnpm lint && pnpm test && pnpm build` — 24 теста, чисто.
- **Компиляция под целевые ОС:** `splitroute-helper` целиком под
  `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`; `privileged/windows.rs`,
  `privileged/linux.rs`, `privileged/client.rs` — изолированной сборкой с
  `-D warnings` (см. ограничения ниже).
- **Реальная упаковка на macOS:** `SplitRoute.app` (96.7 МиБ) и `.dmg`
  (32.8 МиБ); `sing-box` и `splitroute-helper` в `Contents/MacOS/`.
- Скрипты — `sh -n`; bat-скрипты — ASCII + CRLF.

### Не проверено (причина)

- **Установка/удаление служб, `pkexec`, `systemctl`, `sc.exe`, UAC, права
  сокета/pipe, TUN на Linux и Windows** — blocked: нет Linux-машины и Windows
  VM; привилегированные действия автоматически не выполнялись.
- **Сборка `.deb`/`.rpm`/AppImage и NSIS** — blocked: нужны целевые ОС.
- **Успешный запуск macOS LaunchDaemon** — blocked подписью (S3-T2).
- **Сон/пробуждение, смена сети, Chrome/Telegram** — ручная проверка, этап 2.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, sing-box 1.14.4 (pinned).
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** под Windows падает `ring`
  (`assert.h not found`), под Linux — `gio-sys`/`glib-sys` (pkg-config/sysroot).
  Это не ошибка кода; для проверки типов используйте `splitroute-helper` и
  изолированную сборку платформенных модулей.
- `sudo -n true` **не проходит**. Системные сервисы не ставить без явного
  действия пользователя.
- **macOS 15.8.2 + AMFI:** ad-hoc/linker-подпись helper'а не запускается как
  LaunchDaemon (`has no CMS blob`). Нужен сертификат Apple Development или
  Developer ID. `security find-identity -v -p codesigning` сейчас → 0.
- **`externalBin` только в платформенных конфигах.** В базовом
  `tauri.conf.json` он ломает `cargo check` на чистом клоне (sidecar в
  `.gitignore`). Инвариант закреплён `src-tauri/tests/packaging_config.rs`.
- Перед `tauri build` нужны sidecar'ы: `./scripts/fetch-core.sh` и
  `./scripts/build-helper-sidecar.sh`; затем `./scripts/package.sh`.
- Debug-сборка ищет сокет в `$TMPDIR/splitroute-helper.sock`. Для
  установленного helper'а: Linux — `/run/splitroute/helper.sock`, macOS —
  `/var/run/splitroute/helper.sock` (`SPLITROUTE_HELPER_SOCKET`), Windows —
  `\\.\pipe\splitroute-helper` (`SPLITROUTE_HELPER_PIPE`).
- Установщик ищет helper и sing-box: `SPLITROUTE_HELPER_BIN` /
  `SPLITROUTE_SING_BOX_BIN`, рядом с приложением, либо в
  `src-tauri/binaries/sing-box-<triple>` (dev).
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- После проверок: `pgrep -fl 'splitroute-helper|sing-box run'`, `ls -d /tmp/sr-*`,
  `launchctl print system/com.splitroute.helper`.

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

## Ручная проверка S3-T2 (только при наличии сертификата)

**Сейчас blocked:** ad-hoc/linker-подпись helper'а убивается AMFI
(`has no CMS blob`), `security find-identity -v -p codesigning` → 0. Без
сертификата Apple Development/Developer ID успешный сценарий не пройти.
Если сертификат появится — сначала подписать helper:

```bash
codesign --force --sign "Apple Development: <имя>" target/debug/splitroute-helper
codesign -dv --verbose=4 target/debug/splitroute-helper 2>&1 | grep -E 'Signature|flags'
```

В выводе не должно быть `Signature=adhoc`. Дальше:

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
SPLITROUTE_HELPER_SOCKET=/var/run/splitroute/helper.sock pnpm tauri dev
```

1. Настройки → Helper → «Установить», ввести пароль администратора.
2. Проверить:
   ```bash
   launchctl print system/com.splitroute.helper
   ls -ld /var/run/splitroute
   stat -f '%Sp %Su:%Sg %N' /var/run/splitroute/helper.sock
   ```
   Ожидается: сервис `running`; каталог `drwx--x--x root wheel`; сокет
   `srw------- <пользователь>:wheel`.
3. В приложении: статус helper'а с версией sing-box, подключение/отключение,
   при подключении — процесс `sing-box run`.
4. Настройки → Helper → «Удалить». Проверить:
   ```bash
   launchctl print system/com.splitroute.helper  # ошибка
   ls -l /Library/LaunchDaemons/com.splitroute.helper.plist 2>/dev/null || echo "plist удалён"
   ls -l /Library/PrivilegedHelperTools/com.splitroute.helper 2>/dev/null || echo "helper удалён"
   ls -ld /var/run/splitroute 2>/dev/null || echo "runtime удалён"
   pgrep -fl 'splitroute-helper|sing-box run' || echo "процессов нет"
   ```
5. Если не работает: `log show --predicate 'process == "splitroute-helper"'
   --last 5m`, `launchctl print`, `codesign -dv`.

Результат отразить в `TASKS.md` (S3-T2 → done) и в
`docs/stage-3-report.md` §5–§6.

Фактический результат 2026-11-12 (без сертификата): установка скопировала
plist и файлы, но AMFI убила ad-hoc helper; `launchctl print` → «Could not
find service», сокета не было; Uninstall удалил всё, процессов не осталось.

---

## Ручная проверка S3-T3 и S3-T4 (только на целевых ОС)

**Сейчас blocked:** нет Linux-машины и Windows VM. Код собран под обе тройки,
но установка служб, права и TUN не проверялись. Привилегированные действия —
только по явному подтверждению пользователя.

**Важно:** Windows-бинарь нельзя собрать на macOS — `ring` требует MSVC-заголовки,
NSIS не кросс-компилируется. Сборка только на Windows.

### Windows: portable-архив («распаковал и запустил»)

На Windows 10/11 x64 с VS Build Tools, Rust MSVC, Node 22 + pnpm, WebView2 и
**Git for Windows** (скрипты — POSIX sh, запускать в Git Bash):

```bash
cd app
rustup target add x86_64-pc-windows-msvc
./scripts/fetch-core.sh          # sing-box + sha256
./scripts/fetch-wintun.sh        # wintun.dll + sha256
./scripts/portable.sh            # target/portable/SplitRoute-portable-windows-x64.tar.gz
```

Распаковать архив и запустить `start.bat` — он поднимет helper в консоли через
UAC (подкоманда `console`) и откроет приложение. Затем в UI: Настройки → Helper →
«Установить».

Проверки:

```powershell
sc.exe query splitroute-helper-service
Get-ChildItem "$env:ProgramFiles\SplitRoute"
Get-ChildItem "$env:ProgramData\SplitRoute"
```

Ожидается: служба `RUNNING` под SYSTEM; рядом с `sing-box.exe` лежит
`wintun.dll`; pipe доступен только SID пользователя; TUN поднимается и выбранное
приложение идёт через прокси.

### Windows: NSIS-установщик

```bash
./scripts/package.sh     # требует wintun.dll, собирает .exe установщика
```

### Linux

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
./scripts/fetch-core.sh
./scripts/package.sh                                  # .deb / .rpm / AppImage
sudo apt install ./target/release/bundle/deb/*.deb    # установка пакета
# затем в UI: Настройки → Helper → «Установить» (pkexec)
systemctl status splitroute-helper.service
ls -ld /run/splitroute
stat -c '%a %U:%G %n' /run/splitroute/helper.sock
pgrep -a -f 'splitroute-helper|sing-box run'
```

Ожидается: сервис `active (running)`; каталог `711 root:root`; сокет `600`
пользователя; `--allow-uid` = uid установившего. Удаление — кнопкой в UI
(`pkexec`).

### Windows (VM)

```powershell
sc.exe query splitroute-helper-service
Get-ChildItem 'C:\Program Files\SplitRoute'
Get-ChildItem 'C:\ProgramData\SplitRoute'
```

Ожидается: служба `RUNNING` под SYSTEM; рядом с `sing-box.exe` лежит
`wintun.dll`; pipe доступен только SID пользователя; TUN поднимается и выбранное
приложение идёт через прокси.

Результаты — в `TASKS.md` (S3-T3/S3-T4) и `docs/stage-3-report.md` §5.1.

---

## Промпт следующей подцели: G3.3 (S3-T5, S3-T6)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.
S3-T2 **blocked по подписи**; S3-T3/S3-T4 закрыты по коду, ручная проверка —
S3-T7.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G3.3 этапа 3 — S3-T5 (проводка установки helper'а в UI: статус,
ошибки, пароль не хранится) и S3-T6 (THIRD_PARTY_NOTICES, GPL-3.0-обязательства,
LICENSE и notices в бандле).
После подцели остановись: коммит кода, отдельный коммит документации,
обновление NEXT_SESSION.md. Не начинай S3-T7 и S3-T8.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §2, §10–§14, §17–§18.
- PRD.md §6.6, §6.8, §7 (состояния и ошибки), §13.
- TASKS.md: S3-T5, S3-T6, этап 3.
- NEXT_SESSION.md (этот файл), docs/stage-3-report.md (особенно §7 п.5–11),
  docs/research.md §11.
- Текущее состояние: src-tauri/src/privileged/{installer,macos,linux,windows,
  mock,client}.rs; src/features/settings; src/shared/api; src/shared/i18n.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник типов IPC; формат протокола не меняется.
- InstallerError::{RequiresPrivileges,Unsupported,MissingBinary,Failed} — контракт
  установщика; UI должен показывать причину, а не общую строку.
- UI не получает root и не вызывает shell; установка/удаление — только по явному
  действию пользователя с подтверждением (polkit/UAC/osascript).
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- Не ломать macOS/Linux/Windows-ветки и существующие тесты (173 Rust + 24 frontend).

ЗАДАЧИ
1. S3-T5 (UI):
   - `helper_install`/`helper_uninstall` уже вызывают реальный установщик;
     задача — показать пользователю точную причину. Сейчас `errorText` в UI
     подменяет сообщение установщика общей строкой `errors.INSTALLER`.
     Покажи `error.message` (или типизированный код + текст), сохранив
     локализацию; покрой тестами состояния «нет прав / отменено / нет бинаря /
     не поддерживается».
   - Статус helper'а (установлен/не установлен/устарел/не отвечает) — по
     `helper_status`, без хранения пароля.
   - Не менять экраны сверх необходимого; строки — в i18n-словарь (ru).
2. S3-T6 (GPL-3.0):
   - `THIRD_PARTY_NOTICES` в корне: sing-box (GPL-3.0, ссылка на исходники
     pinned-версии), Tauri и плагины, Rust-крейты, иконки/шрифты.
   - Убедиться, что `LICENSE` и `THIRD_PARTY_NOTICES` попадают в бандл
     (Tauri `licenseFile`/`resources`), включая `.deb`/`.rpm`/AppImage и NSIS.
   - Проверить список зависимостей (`cargo tree`, `pnpm licenses list`) и
     зафиксировать лицензии.
3. Обнови тесты и документацию; не начинай S3-T7/S3-T8.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Если менялись конфиги бандла — `./scripts/package.sh` (нужны sidecar'ы);
  не ломать инвариант «`externalBin` только в платформенных конфигах»
  (`src-tauri/tests/packaging_config.rs`).
- Привилегированные действия не выполнять.

ОГРАНИЧЕНИЯ
- root/polkit/UAC — только по явному действию пользователя.
- Секреты не логировать и не коммитить.
- Не менять формат протокола и установщики без необходимости.

ОТЧЁТ
- Обнови TASKS.md (S3-T5, S3-T6) и docs/stage-3-report.md (раздел G3.3).
- NEXT_SESSION.md: состояние и следующий промпт G3.4 (S3-T7, S3-T8).
- Коммиты: feat:/fix:/test: для кода, отдельный docs: для документации.
```

---

## Заметки

- Открытые пункты этапа 3: S3-T5 (UI), S3-T6 (notices/GPL), S3-T7 (smoke на
  чистых системах + CachyOS-сценарий S2-T9), S3-T8 (отчёт и handoff этапа 4).
- **S3-T2 blocked по подписи.** Нужен Apple Development/Developer ID. Установщик
  проверяет подпись до копирования и не оставляет полусобранный сервис.
- **G3.2 не проверен на реальных Linux/Windows.** Код компилируется под обе
  тройки, генераторы покрыты тестами, но установка служб, права сокета/pipe и
  TUN — только в S3-T7. Не выдавать за проверенное.
- **S3-T5:** `errorText` в UI подменяет сообщение установщика общей строкой
  `errors.INSTALLER`, поэтому пользователь не видит причину (например, подпись
  macOS или отсутствующий бинарь). Это первое, что нужно исправить.
- **Упаковка:** `externalBin` только в `tauri.linux.conf.json`/
  `tauri.windows.conf.json`; иначе `cargo check` на чистом клоне падает.
  Перед `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
- **Windows:** `sc.exe` требует `binPath` одним экранированным токеном
  (`cf26d1a`); `TOKEN_USER` читается из буфера `Vec<u64>` (выравнивание).
- Известные ограничения G3.1: osascript-скрипт во временном каталоге (TOCTOU —
  для релиза SMJobBless/Authorization Services или подписанный установщик);
  `--allow-uid`/`--allow-sid` фиксируют одного пользователя.
- Подпись нужна для распространения: Developer ID + notarization (macOS),
  Authenticode/SmartScreen (Windows), подпись пакетов (Linux); `wintun.dll` —
  только официальная подписанная.
