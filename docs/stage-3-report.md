# Отчёт этапа 3 — helper'ы и упаковка

Дата: 2026-11-12. Подцели G3.1: S3-T1 (Rust-helper) и S3-T2 (macOS
LaunchDaemon). Коммиты кода: `d7af3ba`, `d9da641`. G3.2: S3-T3 (Linux) и
S3-T4 (Windows) — коммиты `be1d918`, `684e6e5`, `276f218`, `cf26d1a`.

Короткий вывод: **Rust-helper реализован и проверен** — протокол, права
сокета, проверка uid, `sing-box check`, запуск/остановка реального sing-box
1.14.4 без root и без TUN. **macOS-установщик реализован и проверен до границы
подписи.** Реальная установка LaunchDaemon выполнена: файлы и plist встали
корректно, но macOS 15.8.2 (AMFI) убила helper с ad-hoc/linker-подписью
(`has no CMS blob`), сервис не загрузился, сокета не было. После проверки всё
удалено. На машине нет codesigning identity, поэтому S3-T2 **blocked по
подписи**, а не по коду; установщик теперь проверяет подпись до копирования.
Чек-лист — §6.

**G3.2 (Linux и Windows) реализован по коду и проверен на этой машине тем, что
проверяемо без целевых ОС:** генерация юнита/скриптов/команд, `sh -n`, права,
экранирование, сериализация, плюс `clippy -D warnings` под
`x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`. Установка служб,
`pkexec`, UAC, `systemctl` и `sc.exe` **не выполнялись** — нет Linux-машины и
Windows VM; эти пункты помечены blocked, а не «сделано». Реальная сборка пакета
проверена на macOS (`.app` + `.dmg` со sidecar'ами). Матрица — §5.1.

## 1. Окружение

| Параметр | Значение |
|---|---|
| ОС | macOS 15.8.2 (24H232), arm64 |
| Ядро | sing-box 1.14.4 (pinned, `scripts/core-version.txt`) |
| Rust | 1.99.1, stable |
| Node / pnpm | Node 22, pnpm 11.24.0 |
| Права | обычный пользователь (uid 501); root — только через системный запрос прав установщика |
| Helper | `target/debug/splitroute-helper` (Rust) |
| UI-автоматизация | недоступна |

Секреты (URL подписки, HWID, UUID, адреса серверов) не использовались, не
логировались и не попадали в тесты/отчёт.

## 2. Что реализовано

### 2.1 `crates/helper` (S3-T1)

Бинарь `splitroute-helper` и библиотека `splitroute_helper`.

- **Транспорт.** Unix-сокет, один JSON на строку, лимит
  `MAX_MESSAGE_BYTES` = 1 МиБ, таймаут чтения/записи 10 с.
- **Сокет.** Права `0600`; под root владелец меняется на `--allow-uid`
  (иначе пользователь не подключится к `0600`-сокету). Застарелый сокет
  удаляется, живой второй экземпляр не запускается.
- **Права каталога.** Runtime-каталог под root — `0711` (обход без листинга),
  чтобы пользователь мог дойти до своего сокета; под обычным пользователем —
  `0700`. Конфиг и лог ядра — `0600` root.
- **Аутентификация.** uid пира: `getpeereid` на macOS, `SO_PEERCRED` на Linux.
  `std::os::unix::net::UnixStream::peer_cred` в Rust 1.99 ещё нестабилен, поэтому
  используется libc. Клиент с другим uid получает `FORBIDDEN`.
- **Команды.** Только `status`, `start`, `stop`. `install`/`uninstall` по
  сокету запрещены (`FORBIDDEN`): установка/удаление — задача установщика с
  правами администратора. Неизвестная строка — `UNKNOWN_COMMAND`.
- **Start.** Принимает только объект `payload.config`, сам пишет его атомарно
  в runtime-каталог с правами `0600`, выполняет `sing-box check`, затем
  `sing-box run`. Текст ошибки ядра не отдаётся клиенту (может содержать
  значения конфига). Повторный start — `ALREADY_RUNNING`.
- **Stop.** Идемпотентен; `SIGTERM` ядру, ожидание до 5 с, затем `SIGKILL`;
  конфиг удаляется.
- **Завершение helper'а.** `SIGTERM`/`SIGINT` останавливает ядро, удаляет
  конфиг и сокет. После остановки новые `start` не принимаются.
- **Второй экземпляр.** Не запускается и не удаляет конфиг работающего
  helper'а.
- **Логи.** Содержимое конфига не логируется; наружу — только факт запуска,
  путь сокета и ошибки без секретов.

### 2.2 macOS-установщик (S3-T2)

`src-tauri/src/privileged/macos.rs`, `MacosInstaller`.

| Что | Куда |
|---|---|
| Helper | `/Library/PrivilegedHelperTools/com.splitroute.helper` |
| sing-box | `/Library/Application Support/SplitRoute/sing-box` |
| plist | `/Library/LaunchDaemons/com.splitroute.helper.plist` |
| Runtime/socket | `/var/run/splitroute`, `/var/run/splitroute/helper.sock` |

- Label `com.splitroute.helper`, `RunAtLoad` + `KeepAlive`, аргументы
  `--socket`, `--runtime-dir`, `--sing-box`, `--allow-uid <uid установщика>`.
- Подпись: установщик проверяет helper через `codesign -dv` **до** копирования.
  Ad-hoc/linker-подпись и отсутствие подписи отклоняются с понятной ошибкой:
  macOS 15.8.2 (AMFI) не запускает такой LaunchDaemon. Нужен сертификат
  Apple Development или Developer ID. Ad-hoc подпись из установщика удалена.
- Привилегированные шаги — `osascript` с `administrator privileges`: пароль
  запрашивает система, установщик его не читает и не хранит. Это и есть
  подтверждение действия пользователя.
- Установка: `install -o root -g wheel`, `launchctl bootout/bootstrap/enable/
  kickstart`, затем ожидание сокета до 10 с.
- Удаление: `launchctl bootout`, удаление plist, helper'а, sing-box, каталога
  `/Library/Application Support/SplitRoute` (если пуст) и `/var/run/splitroute`.
- UI не менялся: кнопки «Установить/Удалить» в Настройках уже вызывали
  `helper_install`/`helper_uninstall`; теперь за ними реальный установщик.
  Полировка статусов и ошибок — S3-T5.

## 2.3. Платформенный транспорт helper'а (G3.2)

Протокол не менялся. Транспорт вынесен в платформенные модули `crates/helper`:

| Модуль | Назначение |
|---|---|
| `platform` | права файлов (`0711`/`0600`), признак root, остановка ядра (SIGTERM → SIGKILL на Unix, `kill` на Windows) |
| `handler` | разбор запроса, диспетчеризация, ответ; общий для обоих транспортов |
| `server` | Unix-сокет `0600`, uid пира (`getpeereid`/`SO_PEERCRED`), цикл accept |
| `pipe` | named pipe с SDDL-ACL, сверка SID процесса-клиента |
| `service` | служба Windows под SYSTEM: диспетчер служб, `SERVICE_CONTROL_STOP`, статусы |
| `args` | общий разбор флагов (`--socket`/`--pipe`, `--allow-uid`/`--allow-sid`) |

macOS-ветка сохранила прежнее поведение; существующие тесты helper'а прошли без
изменений ожиданий.

## 2.4. Linux: systemd + polkit (S3-T3)

`src-tauri/src/privileged/linux.rs`, `LinuxInstaller`.

| Что | Куда |
|---|---|
| Helper | `/usr/lib/splitroute/splitroute-helper` |
| sing-box | `/usr/lib/splitroute/sing-box` |
| Юнит | `/etc/systemd/system/splitroute-helper.service` |
| Runtime/socket | `/run/splitroute`, `/run/splitroute/helper.sock` |

- Юнит: `Type=simple`, `User=root`, `RuntimeDirectory=splitroute`,
  `RuntimeDirectoryMode=0711`, `UMask=0077`, `Restart=on-failure`,
  `RestartSec=2`, `WantedBy=multi-user.target`; `ExecStart` — helper с
  фиксированными флагами, `--allow-uid` установившего пользователя.
- Установка/удаление — `pkexec /bin/sh <скрипт>`; пароль запрашивает система,
  установщик его не читает. Код 126 = отмена пользователем, 127 = нет прав
  (`RequiresPrivileges`).
- Скрипты: только абсолютные пути, `set -eu`, идемпотентный
  `systemctl disable --now ... || true`, `daemon-reload`, `enable --now`;
  uninstall удаляет юнит, бинари и `/run/splitroute`.
- `--allow-uid` берётся из `/proc/self/status` (без новой зависимости `libc`).
- Клиент в release-сборке ищет `/run/splitroute/helper.sock` — тот же путь, что
  в юните (раньше был `/var/run/...`; на дистрибутивах с симлинком это одно и
  то же, но лучше не полагаться). Инвариант закреплён тестом.

## 2.5. Windows: служба под SYSTEM, named pipe (S3-T4)

`src-tauri/src/privileged/windows.rs` + `crates/helper/src/{pipe,service}.rs`.

| Что | Куда |
|---|---|
| Helper | `C:\Program Files\SplitRoute\splitroute-helper.exe` |
| sing-box | `C:\Program Files\SplitRoute\sing-box.exe` |
| wintun.dll | `C:\Program Files\SplitRoute\wintun.dll` (рядом с `sing-box.exe`) |
| Runtime/config | `C:\ProgramData\SplitRoute` |
| Pipe | `\\.\pipe\splitroute-helper` |
| Служба | `splitroute-helper-service` под SYSTEM |

- Pipe создаётся с SDDL `D:(A;;GA;;;<SID>)(A;;GA;;;SY)(A;;GA;;;BA)` — только
  пользователь, SYSTEM и Administrators. Дополнительно helper сверяет SID
  процесса-клиента (`GetNamedPipeClientProcessId` → токен → SID), не полагаясь
  только на ACL.
- Служба: `StartServiceCtrlDispatcherW`, `RegisterServiceCtrlHandlerExW`,
  `SERVICE_CONTROL_STOP` → остановка ядра и выход; статусы start pending/running/
  stopped. `run` не блокируется на `ConnectNamedPipe` в главном потоке: ожидание
  клиента живёт в отдельном потоке, поэтому STOP не подвисает.
- Установщик: `sc.exe create ... obj= LocalSystem start= auto`,
  `binPath` передаётся **одним** экранированным токеном (см. §7);
  runtime-каталог закрывается `icacls` (SYSTEM/Administrators полный доступ,
  SID пользователя — чтение); UAC через
  `powershell Start-Process -Verb RunAs -Wait -PassThru`, отказ = код 1223 →
  `RequiresPrivileges`.
- Клиент приложения на Windows теперь `WindowsPipeClient`, а не mock.

## 2.6. Упаковка (S3-T3/S3-T4)

- `tauri.linux.conf.json`: `deb`/`rpm`/`appimage`, зависимости
  `webkit2gtk-4.1`/`gtk3`/`polkit`, `externalBin` = sing-box + helper.
- `tauri.windows.conf.json`: NSIS `perMachine`, `externalBin` = sing-box + helper,
  `resources` = `{"binaries/wintun.dll": "wintun.dll"}`.
- `externalBin` намеренно **отсутствует** в базовом `tauri.conf.json`:
  `tauri-build` ищет sidecar уже на `cargo check`, а бинари в `.gitignore` —
  чистый клон не собрался бы. Инвариант закреплён тестами
  `src-tauri/tests/packaging_config.rs`.
- `scripts/build-helper-sidecar.sh` собирает helper и кладёт его под именем
  `splitroute-helper-<triple>` (Tauri ждёт суффикс тройки);
  `scripts/package.sh` выбирает цели по ОС и проверяет наличие sing-box.
- `scripts/fetch-wintun.sh` качает закреплённую wintun 0.14.1 с проверкой
  SHA256; `scripts/portable.sh` собирает архив «распаковал и запустил».

### 2.7. Дефекты, найденные при подготовке ручной проверки (S3-T4)

Обе ошибки видны только на Windows, но проявлялись бы как «не работает на
чистой машине», поэтому исправлены до передачи на проверку (`3bf04e9`).

1. **`wintun.dll` не попадал в пакет.** Был только `externalBin`, а установщик
   ищет библиотеку рядом с исполняемым файлом — сборка NSIS падала бы в
   `MissingBinary`. На Windows `$RESOURCE` — это каталог экзешника (проверено по
   документации `PathResolver::resource_dir`), поэтому ресурс раскладывается в
   корень, а не в подкаталог. Закреплено тестом
   `windows_config_bundles_wintun_next_to_core`.
2. **Helper умел работать только под диспетчером служб.** Запуск из консоли
   возвращал 1063 (`ERROR_FAILED_SERVICE_CONTROLLER_CONNECT`), хотя AGENTS.md §7
   обещал dev-режим без установки службы. Добавлена подкоманда `console`:
   запуск от администратора без регистрации службы. Разбор подкоманд вынесен в
   `args::split_subcommand`, чтобы тесты выполнялись на macOS (5 тестов).

## 3. Протокол и совместимость

- `crates/protocol` остаётся единственным источником типов: `Command`,
  `Response`, `HelperStatus`, `PROTOCOL_VERSION`, `MAX_MESSAGE_BYTES`.
  Формат не менялся.
- `src-tauri/src/privileged/client.rs` не менялся; добавлен интеграционный
  тест `src-tauri/tests/helper_roundtrip.rs`, который поднимает реальный
  helper и проходит `UnixHelperClient::status/start/stop`.
- Dev-переключатель `SPLITROUTE_MOCK_HELPER=1` сохранён.
- Debug-сборка приложения по-прежнему ищет сокет в `$TMPDIR`; для работы с
  установленным LaunchDaemon нужен
  `SPLITROUTE_HELPER_SOCKET=/var/run/splitroute/helper.sock`.

## 4. Автоматические проверки

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm build
```

Результат на 2026-11-12 (после G3.2 и исправлений S3-T4):

| Набор | Тестов | Результат |
|---|---|---|
| `splitroute` lib (macOS 12 + Linux 13 + Windows 15 установщиков) | 108 | ok |
| `splitroute` golden-конфиги | 4 | ok |
| `splitroute` фикстура подписки | 1 | ok |
| `splitroute` client ↔ helper round-trip | 1 | ok |
| `splitroute` конфигурация упаковки | 7 | ok |
| `splitroute-helper` lib (вкл. 5 тестов подкоманд) | 31 | ok |
| `splitroute-helper` интеграционные | 11 | ok |
| `splitroute-protocol` | 16 | ok |
| **Rust всего** | **179** | **ok** |
| frontend (Vitest) | 24 | ok |

Проверки helper'а покрывают: сериализацию/десериализацию, неизвестную
команду, `install`/`uninstall` по сокету, несовпадение версии, битый JSON,
лимит 1 МиБ, права сокета `0600`, права конфига `0600`, чужой uid,
невалидный конфиг (ядро не запускается), полный start/status/stop,
идемпотентный stop, `SIGTERM` helper'а, второй экземпляр, реальный клиент
приложения. Скрипты установки проверяются `sh -n`, AppleScript — `osacompile`
(без выполнения привилегированных команд).

Дополнительно для G3.2:

- `cargo clippy --workspace --all-targets -- -D warnings` — чисто.
- `cargo check`/`clippy -D warnings` под `x86_64-pc-windows-msvc`:
  `splitroute-helper` целиком; `privileged/windows.rs` и `privileged/client.rs`
  — изолированной сборкой (см. §5.1, почему не через `splitroute`).
- `cargo check`/`clippy -D warnings` под `x86_64-unknown-linux-gnu`:
  `splitroute-helper` целиком; `privileged/linux.rs` и клиент — изолированно.
- Linux: юнит и оба скрипта проверяются `sh -n`; Windows: bat-скрипты —
  ASCII + CRLF (для `cmd.exe` в OEM-кодировке).
- Пакет: реальная сборка `pnpm tauri build` на macOS → `SplitRoute.app`
  (96.7 МиБ) и `.dmg` (32.8 МиБ); `sing-box` и `splitroute-helper` лежат в
  `Contents/MacOS/` рядом с основным бинарём.
- Portable-архив: `./scripts/portable.sh` → `SplitRoute-portable-macos-arm64.tar.gz`
  (33 МиБ). Распакован и проверен: `sing-box version` → 1.14.4,
  `splitroute-helper --help` показывает подкоманды `service`/`console`.

## 5. Ручные проверки

| Проверка | Статус | Чем подтверждено |
|---|---|---|
| Реальный `sing-box check` через helper (1.14.4) | ✅ | временный harness, не коммитился |
| Реальный `sing-box run` (mixed-inbound, без TUN) | ✅ | порт `127.0.0.1` слушает, pid живой |
| Сокет dev-helper'а `0600` | ✅ | `stat` на реальном запуске |
| Конфиг на диске `0600` | ✅ | `stat` во время работы |
| `status` до/после старта, `stop` | ✅ | реальные ответы протокола |
| `stop` завершает ядро и удаляет конфиг | ✅ | `kill(pid, 0)` → ESRCH, файла нет |
| Реальный `UnixHelperClient` ↔ helper | ✅ | `helper_roundtrip.rs` |
| Установка LaunchDaemon (файлы и plist) | ✅ | файлы скопированы `root:wheel`, plist валиден |
| Запуск LaunchDaemon | ❌ **blocked** | ядро: `AMFI: ... has no CMS blob?` / `Unrecoverable CT signature issue`; launchd удалил сервис |
| `launchctl print system/com.splitroute.helper` после установки | ❌ | `Could not find service` |
| Сокет и права `/var/run/splitroute` в prod | ❌ | helper убит до `bind`; каталог `0711 root:daemon` создан |
| Удаление установленных файлов | ✅ | plist, helper, sing-box, runtime удалены; процессов нет |
| Кнопка установки в запущенном UI | ✅ частично | нажатие, пароль и ошибка отработали; успешный запуск blocked подписью |
| Принятие ad-hoc подписи launchd | ❌ **опровергнуто** | AMFI: `has no CMS blob` |
| Linux / Windows (S3-T3/T4) | см. §5.1 | на этой машине нет целевых ОС |

Фактический лог попытки:

```text
AMFI: '/Library/PrivilegedHelperTools/com.splitroute.helper' has no CMS blob?
AMFI: '/Library/PrivilegedHelperTools/com.splitroute.helper': Unrecoverable CT signature issue, bailing out.
```

`security find-identity -v -p codesigning` → `0 valid identities found`.
После неудачной попытки сервис, файлы, `/var/run/splitroute` и процессы
удалены; порты 1420/10808 свободны.

## 5.1. Ручная матрица G3.2 (Linux и Windows)

Окружение: только macOS 15.8.2 arm64. Linux-машины и Windows VM нет, поэтому
всё, что требует целевой ОС, помечено как **не проверено** с причиной. Код при
этом скомпилирован под целевые тройки (§4), а генераторы проверены тестами.

| Проверка | Linux | Windows | Чем подтверждено / почему нет |
|---|---|---|---|
| Генерация юнита / команд службы | ✅ | ✅ | 13 и 15 тестов установщиков |
| `sh -n` юнита и скриптов установки | ✅ | — | тест `generated_shell_scripts_parse` |
| bat-скрипты: ASCII + CRLF | — | ✅ | тест `generated_bat_scripts_are_ascii` |
| Права runtime-каталога (`0711`/ACL) | ✅ (строка в юните и скрипте) | ✅ (строка `icacls`) | тесты содержимого |
| `--allow-uid` при установке | ✅ | — | юнит и `ExecStart` в тесте |
| `--allow-sid`, ACL pipe | — | ✅ | `pipe_sddl`, сверка SID в тесте |
| `binPath` одним экранированным токеном | — | ✅ | тест `service_create_command_quotes_whole_bin_path` |
| Путь сокета совпадает у клиента и юнита | ✅ | — | тест `socket_path_matches_client_release_default` |
| Компиляция под целевую тройку | ✅ `splitroute-helper` целиком; установщик — изолированно | ✅ `splitroute-helper` целиком; установщик и клиент — изолированно | `cargo check`/`clippy` |
| Установка сервиса (`pkexec`/UAC) | ❌ **blocked** | ❌ **blocked** | нет Linux/Windows; запрещено выполнять автоматически |
| `systemctl enable --now`, запуск helper | ❌ **blocked** | — | нет Linux-машины |
| `sc.exe create/start`, служба под SYSTEM | — | ❌ **blocked** | нет Windows VM |
| Права `/run/splitroute` и сокета в prod | ❌ **blocked** | — | нет Linux-машины |
| Доступ к pipe с чужого SID | — | ❌ **blocked** | нужно две учётные записи на Windows |
| TUN через службу, `wintun.dll` | — | ❌ **blocked** | нет Windows VM; `wintun.dll` теперь попадает в пакет (`3bf04e9`), но фактический TUN на Windows проверен в S3-T7; на macOS TUN проверен в S2-T8 |
| Portable-архив распаковывается и стартует | ✅ | `portable.sh` на macOS: sing-box 1.14.4, helper с `service`/`console`; Windows-архив не собирался |
| Сборка `.deb`/`.rpm`/AppImage | ❌ **blocked** | — | нужен Linux; `linuxdeploy`/`fakeroot` не ставились |
| Сборка NSIS | — | ❌ **blocked** | нужен Windows; кросс-сборка не поддерживается |
| Сборка `.app` + `.dmg` со sidecar | — | — | ✅ на macOS: 96.7 МиБ и 32.8 МиБ |
| Sidecar'ы в `Contents/MacOS/` | — | — | ✅ `sing-box` и `splitroute-helper` на месте |

Про кросс-проверку: `cargo check -p splitroute --target x86_64-pc-windows-msvc`
падает на `ring` (`assert.h not found` — нет MSVC-заголовков), а под Linux — на
`gio-sys`/`glib-sys` (`pkg-config has not been configured to support
cross-compilation`). Это ограничение окружения, а не кода. Чтобы типы всё же
проверялись, `windows.rs`, `linux.rs` и `client.rs` собирались изолированно
(временный crate с копией трейта `HelperInstaller`/`HelperClient`) под свою
тройку с `-D warnings`; временные crate'ы удалены.

`pkg-config`/sysroot на этой машине не настраивались сознательно: частичная
кросс-сборка Tauri всё равно не даёт проверяемого результата без целевой ОС.

## 6. Ручная проверка S3-T2 (выполнить с паролем администратора)

**Предусловие:** helper должен быть подписан сертификатом Apple Development
или Developer ID. Ad-hoc/linker-подпись не подходит — AMFI убьёт процесс.

```bash
codesign -dv --verbose=4 target/debug/splitroute-helper 2>&1 | grep -E 'Signature|flags'
```

Если видно `Signature=adhoc`, установщик отклонит установку до копирования
файлов. Подписать helper можно так (после получения сертификата):

```bash
codesign --force --sign "Apple Development: <имя>" target/debug/splitroute-helper
```

Собрать helper и запустить приложение:

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p splitroute-helper
SPLITROUTE_HELPER_SOCKET=/var/run/splitroute/helper.sock pnpm tauri dev
```

В приложении: Настройки → Helper → «Установить», ввести пароль администратора
(его запрашивает macOS, приложение его не хранит). Затем:

```bash
launchctl print system/com.splitroute.helper
ls -ld /var/run/splitroute
stat -f '%Sp %Su:%Sg %N' /var/run/splitroute/helper.sock
```

Ожидается: сервис в состоянии `running`; каталог `drwx--x--x root wheel`;
сокет `srw------- <пользователь>:wheel`. В приложении — статус helper'а с
версией sing-box, подключение/отключение работают, при подключении есть
процесс `sing-box run`.

Удаление — Настройки → Helper → «Удалить». После:

```bash
launchctl print system/com.splitroute.helper   # должен завершиться ошибкой
ls -l /Library/LaunchDaemons/com.splitroute.helper.plist 2>/dev/null || echo "plist удалён"
ls -l /Library/PrivilegedHelperTools/com.splitroute.helper 2>/dev/null || echo "helper удалён"
ls -ld /var/run/splitroute 2>/dev/null || echo "runtime удалён"
pgrep -fl 'splitroute-helper|sing-box run' || echo "процессов нет"
```

Если установка не удалась: `log show --predicate 'process ==
"splitroute-helper"' --last 5m` (ищите `AMFI` и `no CMS blob`),
`launchctl print system/com.splitroute.helper`,
`codesign -dv /Library/PrivilegedHelperTools/com.splitroute.helper`.

## 7. Известные ограничения и риски

1. **osascript и временный скрипт.** Привилегированный shell-скрипт лежит в
   пользовательском временном каталоге между записью и запуском от root. Для
   dev/MVP это приемлемо, но для релиза нужен SMJobBless/Authorization
   Services или подписанный установщик: иначе остаётся TOCTOU-риск для
   скомпрометированной сессии того же пользователя.
2. **Подпись helper'а (R7).** Ad-hoc/linker-подпись не годится для LaunchDaemon
   на macOS 15.8.2: AMFI убивает процесс (`has no CMS blob`). Нужен сертификат
   Apple Development/Developer ID; без него установка блокируется. Установщик
   проверяет подпись до копирования. Релиз — Developer ID + notarization.
3. **Один пользователь.** `--allow-uid` фиксируется при установке; сервис
   обслуживает только установившего пользователя. Многопользовательский
   режим не поддерживается.
4. **Нет подписки на логи ядра.** В протоколе нет команды чтения логов;
   приложение показывает свои логи. Лог ядра лежит в runtime-каталоге `0600`
   root. Отдельная задача — при появлении требования.
5. **Упаковка.** `bundle.active=false` в базовом конфиге, `externalBin` — только
   в `tauri.linux.conf.json`/`tauri.windows.conf.json`. Причина: `tauri-build`
   ищет sidecar уже на `cargo check`, а бинари в `.gitignore`; в базовом
   конфиге `externalBin` сломал бы сборку на чистом клоне. Сборка пакета —
   только на целевой ОС (`scripts/package.sh`); кросс-сборка установщиков
   Tauri не поддерживает. Sidecar'ы обязательно собирать до `tauri build`
   (`scripts/build-helper-sidecar.sh`), иначе бандл не соберётся.
6. **KeepAlive без backoff.** При постоянной ошибке конфигурации launchd будет
   перезапускать helper; systemd — `Restart=on-failure` с `RestartSec=2`.
   Отдельная политика перезапуска не реализована.
7. **Версия sing-box не проверяется установщиком.** Ожидается pinned-бинарь из
   `scripts/fetch-core.sh`; проверка версии — в упаковке/CI.
8. **Linux/Windows не проверены на реальных ОС.** Установка, запуск службы,
   `pkexec`, UAC, права сокета/pipe и TUN — blocked: нет Linux-машины и Windows
   VM. Код компилируется под обе тройки, генераторы покрыты тестами, но это не
   заменяет smoke. Закрывается в S3-T7.
9. **Windows TUN не проверен.** `wintun.dll` кладётся рядом с `sing-box.exe`
   (этого требует ядро), но реальный TUN и права службы под SYSTEM на Windows
   не запускались.
10. **`sc.exe` и пробелы в путях (исправлено).** `binPath` должен быть одним
    экранированным токеном, иначе `sc.exe` отвергает `service` как параметр.
    Исправлено в `cf26d1a` и закреплено тестом; на живой Windows не проверено.
11. **Release-сборка Windows не запускалась.** `cargo build` под MSVC на macOS
    невозможен (нет MSVC-заголовков и линковщика); CI — на Windows-раннере.

## 8. Следующие шаги

- **S3-T2 blocked по подписи:** для закрытия нужен сертификат Apple
  Development/Developer ID. После подписи helper'а — пройти §6. Код и тесты
  готовы; установщик больше не оставляет полусобранный сервис.
- **S3-T3 и S3-T4 закрыты по коду** (G3.2, коммиты `be1d918`, `684e6e5`,
  `276f218`, `cf26d1a`); ручная проверка на Linux и Windows — в S3-T7.
- G3.3: S3-T5 (полировка установки в UI: `errorText` подменяет сообщение
  установщика общей строкой `errors.INSTALLER`, пользователь не видит причину),
  S3-T6 (notices/GPL-3.0, `LICENSE` и `THIRD_PARTY_NOTICES` в бандле).
- Реальный smoke на чистых системах — S3-T7 (Linux и Windows обязательны:
  сейчас это единственный не проверенный участок G3.2).
