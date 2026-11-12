# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 1–2 закрыты. **Этап 3: G3.1, G3.2 и G3.3 выполнены.**
- **S3-T1 (Rust-helper) — done.** Коммит `d7af3ba`.
- **S3-T2 (macOS LaunchDaemon) — blocked по подписи.** Код готов (`d9da641`),
  реальная установка проверена до границы AMFI: ad-hoc/linker-подпись helper'а
  убивается (`has no CMS blob`), сертификата на машине нет.
- **S3-T3 (Linux: systemd + polkit) — done по коду.** `be1d918`, `684e6e5`,
  `276f218`. Ручная проверка — blocked (нет Linux-машины).
- **S3-T4 (Windows: служба под SYSTEM) — done по коду.** `be1d918`,
  `684e6e5`, `276f218`, `cf26d1a`. Ручная проверка — blocked (нет Windows VM).
- **S3-T5 (установка helper'а в UI) — done.** Ошибки установщика показываются
  с причиной (`INSTALLER_*`), статус — из `helper_status`, пароль не хранится.
- **S3-T6 (лицензии) — done.** `THIRD_PARTY_NOTICES` в корне; `LICENSE` и
  notices в бандле (macOS/DMG проверено сборкой).
- Ветка: `main`. Отчёт: [docs/stage-3-report.md](./docs/stage-3-report.md).
  Факты: [docs/research.md §10–12](./docs/research.md).
- Системные сервисы, сокеты, пайпы и процессы после проверок не остались.

### Что работает

- **Helper, оба транспорта.** `crates/helper`: Unix-сокет `0600` (uid пира
  `getpeereid`/`SO_PEERCRED`) и Windows named pipe
  `\\.\pipe\splitroute-helper` с SDDL-ACL только на SID пользователя, SYSTEM и
  Administrators плюс сверка SID процесса-клиента. Протокол не менялся:
  `status`/`start`/`stop`, `install`/`uninstall` по IPC запрещены.
- **Установщики.** macOS (`/Library/PrivilegedHelperTools`, LaunchDaemon,
  `osascript`), Linux (`/usr/lib/splitroute`, systemd-юнит `0711`/`UMask=0077`,
  `pkexec`), Windows (`C:\Program Files\SplitRoute`, служба под SYSTEM,
  named pipe ACL, `icacls`, UAC). macOS и Linux после установки ждут сокет.
- **UI установки (S3-T5).** Коды `INSTALLER_PRIVILEGES`, `INSTALLER_UNSUPPORTED`,
  `INSTALLER_MISSING_BINARY`, `INSTALLER_FAILED`; `ErrorBox` показывает
  локализованный заголовок и точный текст установщика. Состояния helper'а:
  установлен / не установлен / устарел (`HELPER_PROTOCOL`) / не отвечает
  (`HELPER_TIMEOUT`/`HELPER_UNAVAILABLE`) — `src/shared/lib/helperState.ts`.
  Кнопки «Установить»/«Переустановить»/«Удалить», busy и уведомление об успехе;
  статус после действия берётся из `helper_status` (`refreshHelperStatus`).
- **Лицензии (S3-T6).** `THIRD_PARTY_NOTICES` в корне: sing-box 1.14.4
  (GPL-3.0-or-later, ссылки на исходники тега `v1.14.4`), wintun 0.14.1
  (Prebuilt Binaries License, полный текст), Tauri и плагины, инвентарь Rust
  по семействам, frontend, шрифты/иконки. В бандле: macOS/DMG — проверено;
  `.deb`/`.rpm`/AppImage — `/usr/share/doc/splitroute/`; NSIS — `licenseFile`.
- **Упаковка.** `tauri.linux.conf.json` (deb/rpm/appimage),
  `tauri.windows.conf.json` (NSIS perMachine), `externalBin` только там;
  `scripts/build-helper-sidecar.sh`, `scripts/package.sh`, `scripts/portable.sh`.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-12)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто.
- `cargo test --workspace` — **183 теста**: `splitroute` 109 (макос/Linux/
  Windows-установщики + команды) + golden 4 + фикстура 1 + round-trip 1 +
  упаковка 10, `splitroute-helper` 31 + 11 интеграционных,
  `splitroute-protocol` 16.
- `pnpm typecheck && pnpm lint && pnpm test && pnpm build` — **35 тестов**, чисто.
- **Сборка пакета на macOS:** `./scripts/package.sh` → `SplitRoute.app`
  (96.8 МиБ) и `.dmg` (32.9 МиБ); `LICENSE` и `THIRD_PARTY_NOTICES` лежат в
  `Contents/Resources/`; DMG при открытии показывает текст GPL-3.0 и требует
  принятия лицензии.

### Не проверено (причина)

- **Установка/удаление служб, `pkexec`, `systemctl`, `sc.exe`, UAC, права
  сокета/pipe, TUN на Linux и Windows** — blocked: нет Linux-машины и Windows
  VM; привилегированные действия автоматически не выполнялись.
- **Сборка `.deb`/`.rpm`/AppImage и NSIS** — blocked: нужны целевые ОС. Состав
  (`/usr/share/doc/splitroute/*`, `licenseFile`) закреплён тестами конфига.
- **Успешный запуск macOS LaunchDaemon** — blocked подписью (S3-T2).
- **Сон/пробуждение, смена сети, Chrome/Telegram** — ручная проверка, этап 2.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
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
  `bundle.resources` в базовом конфиге — можно: `LICENSE` и
  `THIRD_PARTY_NOTICES` лежат в git.
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
- **`THIRD_PARTY_NOTICES` обновляется вручную** по разделу 11 файла:
  `cargo metadata --all-features` и `pnpm licenses list --prod`.
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

Распаковать архив (в нём уже `LICENSE` и `THIRD_PARTY_NOTICES`) и запустить
`start.bat` — он поднимет helper в консоли через UAC (подкоманда `console`) и
откроет приложение. Затем в UI: Настройки → Helper → «Установить».

Проверки:

```powershell
sc.exe query splitroute-helper-service
Get-ChildItem "$env:ProgramFiles\SplitRoute"
Get-ChildItem "$env:ProgramData\SplitRoute"
```

Ожидается: служба `RUNNING` под SYSTEM; рядом с `sing-box.exe` лежит
`wintun.dll`, а в каталоге приложения — `LICENSE` и `THIRD_PARTY_NOTICES`;
pipe доступен только SID пользователя; TUN поднимается и выбранное приложение
идёт через прокси.

### Windows: NSIS-установщик

```bash
./scripts/package.sh     # требует wintun.dll, собирает .exe установщика
```

При установке проверить, что NSIS показывает страницу лицензии и что после
установки `LICENSE` и `THIRD_PARTY_NOTICES` лежат в каталоге приложения.

### Linux

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
./scripts/fetch-core.sh
./scripts/package.sh                                  # .deb / .rpm / AppImage
dpkg-deb -c target/release/bundle/deb/*.deb | grep share/doc    # LICENSE и notices
sudo apt install ./target/release/bundle/deb/*.deb    # установка пакета
# затем в UI: Настройки → Helper → «Установить» (pkexec)
systemctl status splitroute-helper.service
ls -ld /run/splitroute
stat -c '%a %U:%G %n' /run/splitroute/helper.sock
pgrep -a -f 'splitroute-helper|sing-box run'
```

Ожидается: пакет содержит `/usr/share/doc/splitroute/{LICENSE,THIRD_PARTY_NOTICES}`;
сервис `active (running)`; каталог `711 root:root`; сокет `600` пользователя;
`--allow-uid` = uid установившего. Удаление — кнопкой в UI (`pkexec`).

Результаты — в `TASKS.md` (S3-T3/S3-T4) и `docs/stage-3-report.md` §5.1.

---

## Промпт следующей подцели: G3.4 (S3-T7, S3-T8)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.
S3-T2 **blocked по подписи**; S3-T3/T4/T5/T6 закрыты.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G3.4 этапа 3 — S3-T7 (smoke на чистых системах: macOS, CachyOS/
Linux, Windows; сценарий S2-T9) и S3-T8 (отчёт этапа 3 и handoff этапа 4).
После подцели остановись: коммит кода/скриптов (если понадобятся), отдельный
коммит документации, обновление NEXT_SESSION.md. Этап 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §7, §9, §10–§14, §17–§18.
- PRD.md §6.6, §6.8, §7, §13.
- TASKS.md: S3-T7, S3-T8, этап 3.
- NEXT_SESSION.md (этот файл), docs/stage-3-report.md §5–§8,
  docs/research.md §10–12.

ТЕКУЩЕЕ СОСТОЯНИЕ
- S3-T1, S3-T3, S3-T4, S3-T5, S3-T6 — done. S3-T2 — blocked по подписи
  (ad-hoc LaunchDaemon убивается AMFI; нужен Apple Development/Developer ID).
- Проверено на macOS: 183 Rust + 35 frontend тестов, `clippy -D warnings`,
  сборка `.app` + `.dmg` с `LICENSE` и `THIRD_PARTY_NOTICES` в Resources.
- Не проверено: реальная установка служб на Linux/Windows, `.deb`/`.rpm`/
  AppImage, NSIS, TUN на Windows, `sc.exe`, `pkexec`, UAC.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник типов IPC; формат не меняется.
- InstallerError::{RequiresPrivileges,Unsupported,MissingBinary,Failed} и
  коды CommandError INSTALLER_* — контракт UI.
- UI не получает root и не вызывает shell; привилегированные действия — только
  по явному действию пользователя.
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- externalBin — только в платформенных конфигах (`packaging_config.rs`).
- LICENSE и THIRD_PARTY_NOTICES обязаны оставаться в бандле.

ЗАДАЧИ
1. S3-T7 — smoke по матрице AGENTS.md §13 на доступных системах.
   - macOS (есть): установка helper'а (blocked подписью — зафиксировать),
     подключение/отключение через dev-helper, выбранное приложение через
     прокси, невыбранное напрямую, DNS, сон/пробуждение, смена сети, выход без
     зависших TUN/процессов.
   - Linux/CachyOS и Windows: если машины нет — явно пометить blocked с
     причиной и точным чек-листом команд; не выдавать за проверенное.
     Для Linux проверить содержимое пакета: `/usr/share/doc/splitroute/*`,
     юнит, `--allow-uid`; для Windows — NSIS-страница лицензии, файлы в
     каталоге приложения, служба под SYSTEM, `wintun.dll` рядом с ядром.
   - S2-T9 (CachyOS): systemd-resolved, TUN, allowlist.
   - Привилегированные действия — только с явным подтверждением пользователя;
     после проверок остановить сервисы, удалить TUN, проверить `pgrep`.
2. S3-T8 — отчёт этапа 3: свести G3.1–G3.4, матрицу проверок по ОС, риски,
   статус S3-T2 (blocked), итог по лицензиям; обновить TASKS.md и
   NEXT_SESSION.md с промптом этапа 4.
3. Не менять протокол, установщики и экраны без необходимости.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- ./scripts/package.sh на целевой ОС (Linux/Windows), если машина есть.

ОГРАНИЧЕНИЯ
- root/polkit/UAC — только по явному действию пользователя.
- Секреты не логировать и не коммитить.
- Не выдавать blocked-пункты за проверенные.

ОТЧЁТ
- TASKS.md: S3-T7, S3-T8 и статус этапа 3.
- docs/stage-3-report.md: раздел G3.4 и итоговая матрица.
- NEXT_SESSION.md: состояние и промпт этапа 4.
- Коммиты: feat:/fix:/test: для кода, отдельный docs: для документации.
```

---

## Заметки

- Открытые пункты этапа 3: S3-T7 (smoke на чистых системах + CachyOS-сценарий
  S2-T9), S3-T8 (отчёт и handoff этапа 4).
- **S3-T2 blocked по подписи.** Нужен Apple Development/Developer ID. Установщик
  проверяет подпись до копирования и не оставляет полусобранный сервис.
- **S3-T5:** причина отказа установщика теперь видна (`INSTALLER_*` + текст в
  `ErrorBox`); статус helper'а — только из `helper_status`. Старого кода
  `errors.INSTALLER` больше нет.
- **S3-T6:** `THIRD_PARTY_NOTICES` обновляется вручную (раздел 11 файла). При
  обновлении sing-box/wintun менять версии и ссылки в разделах 2–3, иначе
  `packaging_config`-тест на `v1.14.4` упадёт — это намеренно.
- **Упаковка:** `externalBin` только в `tauri.linux.conf.json`/
  `tauri.windows.conf.json`; `bundle.resources` можно и в базовом конфиге.
  Перед `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
- **Linux/Windows smoke** требует целевых машин; кросс-сборка Tauri
  установщиков не поддерживается.
- Известные ограничения G3.1: osascript-скрипт во временном каталоге (TOCTOU —
  для релиза SMJobBless/Authorization Services или подписанный установщик);
  `--allow-uid`/`--allow-sid` фиксируют одного пользователя.
- Подпись нужна для распространения: Developer ID + notarization (macOS),
  Authenticode/SmartScreen (Windows), подпись пакетов (Linux); `wintun.dll` —
  только официальная подписанная.
