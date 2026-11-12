# Отчёт этапа 3 — helper'ы и упаковка

Дата: 2026-11-12. Подцель G3.1: S3-T1 (Rust-helper) и S3-T2 (macOS
LaunchDaemon). Коммиты кода: `d7af3ba`, `d9da641`.

Короткий вывод: **Rust-helper реализован и проверен** — протокол, права
сокета, проверка uid, `sing-box check`, запуск/остановка реального sing-box
1.14.4 без root и без TUN. **macOS-установщик реализован и проверен до границы
подписи.** Реальная установка LaunchDaemon выполнена: файлы и plist встали
корректно, но macOS 15.8.2 (AMFI) убила helper с ad-hoc/linker-подписью
(`has no CMS blob`), сервис не загрузился, сокета не было. После проверки всё
удалено. На машине нет codesigning identity, поэтому S3-T2 **blocked по
подписи**, а не по коду; установщик теперь проверяет подпись до копирования.
Чек-лист — §6.

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

Результат на 2026-11-12:

| Набор | Тестов | Результат |
|---|---|---|
| `splitroute` lib (вкл. 12 тестов macOS-установщика) | 78 | ok |
| `splitroute` golden-конфиги | 4 | ok |
| `splitroute` фикстура подписки | 1 | ok |
| `splitroute` client ↔ helper round-trip | 1 | ok |
| `splitroute-helper` lib | 19 | ok |
| `splitroute-helper` args | 4 | ok |
| `splitroute-helper` интеграционные | 11 | ok |
| `splitroute-protocol` | 16 | ok |
| **Rust всего** | **134** | **ok** |
| frontend (Vitest) | 24 | ok |

Проверки helper'а покрывают: сериализацию/десериализацию, неизвестную
команду, `install`/`uninstall` по сокету, несовпадение версии, битый JSON,
лимит 1 МиБ, права сокета `0600`, права конфига `0600`, чужой uid,
невалидный конфиг (ядро не запускается), полный start/status/stop,
идемпотентный stop, `SIGTERM` helper'а, второй экземпляр, реальный клиент
приложения. Скрипты установки проверяются `sh -n`, AppleScript — `osacompile`
(без выполнения привилегированных команд).

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
| Linux / Windows | ⏭ вне G3.1 | S3-T3, S3-T4 |

Фактический лог попытки:

```text
AMFI: '/Library/PrivilegedHelperTools/com.splitroute.helper' has no CMS blob?
AMFI: '/Library/PrivilegedHelperTools/com.splitroute.helper': Unrecoverable CT signature issue, bailing out.
```

`security find-identity -v -p codesigning` → `0 valid identities found`.
После неудачной попытки сервис, файлы, `/var/run/splitroute` и процессы
удалены; порты 1420/10808 свободны.

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
5. **Упаковка.** `bundle.active=false`, sidecar/externalBin не настроены;
   установщик ищет helper рядом с приложением или по
   `SPLITROUTE_HELPER_BIN`, sing-box — по `SPLITROUTE_SING_BOX_BIN`, рядом
   или в `src-tauri/binaries`. Включение в бандл — S3-T6/упаковка.
6. **KeepAlive без backoff.** При постоянной ошибке конфигурации launchd будет
   перезапускать helper; отдельная политика перезапуска не реализована.
7. **Версия sing-box не проверяется установщиком.** Ожидается pinned-бинарь из
   `scripts/fetch-core.sh`; проверка версии — в упаковке/CI.

## 8. Следующие шаги

- **S3-T2 blocked по подписи:** для закрытия нужен сертификат Apple
  Development/Developer ID. После подписи helper'а — пройти §6. Код и тесты
  готовы; установщик больше не оставляет полусобранный сервис.
- G3.2: S3-T3 (Linux systemd + polkit, `.deb`/`.rpm`/AppImage) и S3-T4
  (Windows-служба, named pipe ACL, `wintun.dll`). Там нет требования AMFI;
  подпись нужна только для распространения (SmartScreen/Authenticode).
- G3.3: S3-T5 (полировка установки в UI), S3-T6 (notices/GPL).
- Реальный smoke на чистых системах — S3-T7.
