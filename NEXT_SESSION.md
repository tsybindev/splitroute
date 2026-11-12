# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 1–2 закрыты. **Этап 3, подцель G3.1 выполнена по коду.**
- **S3-T1 (Rust-helper) — done.** Коммит `d7af3ba`. Бинарь
  `splitroute-helper`: Unix-сокет `0600`, uid пира, `status/start/stop`,
  `sing-box check`, остановка ядра по SIGTERM, защита от второго экземпляра.
- **S3-T2 (macOS LaunchDaemon) — код готов, ручная проверка pending.**
  `MacosInstaller` ставит helper в `/Library/PrivilegedHelperTools`, sing-box в
  `/Library/Application Support/SplitRoute`, plist в `/Library/LaunchDaemons`,
  сокет `/var/run/splitroute/helper.sock`. Реальная установка сервиса в этой
  сессии **не выполнялась**: `sudo -n` требует пароль, системный сервис не
  ставился без явного подтверждения. Чек-лист — `docs/stage-3-report.md` §6.
- Ветка: `main`. Отчёт: [docs/stage-3-report.md](./docs/stage-3-report.md).
  Факты: [docs/research.md §10](./docs/research.md).
- Системные сервисы, сокеты и процессы после проверок не остались.

### Что работает

- **Helper.** `crates/helper`: `status`, `start`, `stop`; `install`/`uninstall`
  по сокету запрещены. Лимит сообщения 1 МиБ, версия протокола, JSON по одной
  строке, таймаут 10 с. Конфиг пишется сам в runtime-каталог с правами `0600`,
  проверяется `sing-box check`, содержимое конфига не логируется. `SIGTERM`
  helper'а останавливает ядро и удаляет сокет; второй экземпляр не запускается
  и не удаляет конфиг работающего.
- **Права.** Сокет `0600`, под root владелец — `--allow-uid`; runtime-каталог
  под root `0711` (обход без листинга), конфиг/лог `0600` root. Проверка uid
  пира: `getpeereid` (macOS), `SO_PEERCRED` (Linux).
- **macOS-установщик.** `MacosInstaller`: копирование бинарей, ad-hoc подпись,
  plist, `launchctl bootout/bootstrap/enable/kickstart`, удаление сервиса и
  runtime-каталога. Привилегированные шаги — `osascript` с системным запросом
  прав; пароль не хранится.
- **UI.** Кнопки «Установить/Удалить» в Настройках уже вызывали
  `helper_install`/`helper_uninstall`; теперь за ними реальный установщик.
  Полировка статусов/ошибок — S3-T5, экраны не менялись.
- **Совместимость.** Формат протокола не менялся; `UnixHelperClient` работает
  с новым helper'ом (round-trip тест). `SPLITROUTE_MOCK_HELPER=1` сохранён.

### Проверено (2026-11-12)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто.
- `cargo test --workspace` — 131 тест: `splitroute` 75 + golden 4 + фикстура 1 +
  round-trip 1, helper 19 + args 4 + интеграционные 11, protocol 16.
- `pnpm typecheck && pnpm lint && pnpm test && pnpm build` — 24 теста, чисто.
- Скрипты установщика проверены `sh -n`, AppleScript — `osacompile`
  (без выполнения привилегированных команд).
- **Реальный sing-box 1.14.4 без root и TUN:** `check` проходит, `run` с
  mixed-inbound слушает `127.0.0.1`, `status` отдаёт pid/версию, `stop`
  завершает ядро, конфиг удаляется. Сокет и конфиг — `0600`.
- **Реальный `UnixHelperClient` ↔ helper:** `status/start/stop` по сокету.
- Второй экземпляр helper'а завершается с ошибкой и не удаляет конфиг
  работающего.

### Не проверено (причина)

- **Установка LaunchDaemon, `launchctl print system/com.splitroute.helper`,
  права `/var/run/splitroute`, удаление сервиса** — нужен пароль администратора;
  сервис не ставился без явного подтверждения.
- **Кнопка установки в запущенном приложении** — нет UI-автоматизации.
- **Принятие ad-hoc подписи launchd** — нужен реальный запуск сервиса.
- **Linux и Windows** — S3-T3, S3-T4 (следующая подцель).
- **Сон/пробуждение, смена сети, Chrome/Telegram** — ручная проверка, этап 2.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, sing-box 1.14.4 (pinned).
- `sudo -n true` **не проходит**: нужен пароль. Системные сервисы не ставить
  без явного действия пользователя.
- Helper собирается отдельно: `cargo build -p splitroute-helper` →
  `target/debug/splitroute-helper`.
- Debug-сборка приложения ищет сокет в `$TMPDIR/splitroute-helper.sock`.
  Для установленного LaunchDaemon:
  `SPLITROUTE_HELPER_SOCKET=/var/run/splitroute/helper.sock`.
- Установщик ищет helper и sing-box: `SPLITROUTE_HELPER_BIN` /
  `SPLITROUTE_SING_BOX_BIN`, рядом с приложением, либо в
  `src-tauri/binaries/sing-box-<triple>` (dev).
- Сокет prod: `/var/run/splitroute/helper.sock`, каталог `0711` root, сокет
  `0600` пользователя; конфиг/лог ядра — `0600` root.
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
launchctl print system/com.splitroute.helper 2>/dev/null || echo "сервиса нет"
pgrep -fl 'splitroute-helper|sing-box run' || echo "процессов нет"
```

---

## Ручная проверка S3-T2 (перед следующей подцелью, если есть возможность)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p splitroute-helper
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

---

## Промпт следующей подцели: G3.2 (S3-T3, S3-T4)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.
Сначала, если возможно, закройте ручную проверку S3-T2 выше — она не блокирует
G3.2, но статус в `TASKS.md` должен быть честным.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G3.2 этапа 3 — S3-T3 (Linux: systemd + polkit, .deb/.rpm/AppImage)
и S3-T4 (Windows: служба под SYSTEM, named pipe ACL, wintun.dll).
После подцели остановись: коммит кода, отдельный коммит документации,
обновление NEXT_SESSION.md. Не начинай S3-T5 и S3-T6.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §5, §7, §8, §10–§14, §17–§18.
- PRD.md §6.8, §9, §10 (R7–R9), §13.
- TASKS.md: S3-T3, S3-T4, этапы 2–3.
- NEXT_SESSION.md (этот файл), docs/stage-3-report.md, docs/research.md §10.
- Текущее состояние: crates/helper (Unix), crates/protocol (IPC),
  src-tauri/src/privileged/{client,installer,macos,mock,mod}.rs.
- S3-T1 done; S3-T2 код готов, ручная проверка LaunchDaemon может быть pending.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник типов IPC: Request/Response,
  Command::{Install,Uninstall,Start,Stop,Status}, PROTOCOL_VERSION=1,
  MAX_MESSAGE_BYTES=1 МиБ, HelperStatus.
- Протокол: один JSON на строку; ответ {protocol,id,ok,payload|error};
  таймаут 10 с. Формат не меняется.
- Helper принимает только фиксированные команды; install/uninstall выполняет
  установщик с правами администратора, не сокет.
- Linux: сокет /run/splitroute/helper.sock, каталог 0711 root, сокет 0600
  пользователя, проверка uid пира через SO_PEERCRED.
- Windows: named pipe \\.\pipe\splitroute-helper, ACL только на SID
  пользователя, проверка клиента; служба под SYSTEM.
- UI не получает root и не вызывает shell; установка/удаление — только по
  явному действию пользователя с подтверждением (polkit/UAC).
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- Не ломать macOS-ветку и существующие тесты.

ЗАДАЧИ
1. S3-T3 (Linux):
   - вынести транспорт helper'а в платформенные модули, не меняя протокол;
   - systemd-юнит splitroute-helper.service: RuntimeDirectory=splitroute,
     RuntimeDirectoryMode=0711, UMask=0077, User=root, перезапуск;
   - установка/удаление через pkexec/polkit с подтверждением, передача
     --allow-uid текущего пользователя, пути бинарей — только из установщика;
   - конфигурация сборки .deb/.rpm/AppImage и включение helper'а/sing-box в
     пакет; не коммитить бинарники;
   - тесты: генерация юнита/скриптов, сериализация, ошибки, права; `sh -n`.
2. S3-T4 (Windows):
   - служба под SYSTEM, named pipe с ACL на SID пользователя, проверка клиента;
   - установка/удаление через UAC, пути wintun.dll рядом с sing-box;
   - cfg-модули и тесты, которые можно прогнать на macOS (генерация,
     сериализация, ошибки); Windows-специфичный код — `cargo check` под target.
3. Сохранить dev-переключатель SPLITROUTE_MOCK_HELPER=1 и macOS-поведение.
4. Не устанавливать сервисы и не запускать polkit/UAC автоматически; реальная
   проверка — только по явному действию пользователя.
5. Не менять UI-экраны и не начинать S3-T5/S3-T6.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Linux: cargo check --target x86_64-unknown-linux-gnu (если target установлен);
  сборка .deb/.rpm/AppImage — если есть инструменты; установка/удаление — только
  с явного разрешения пользователя.
- Windows: cargo check --target x86_64-pc-windows-msvc (если target установлен);
  служба/пайп — только на VM и с явного разрешения.
- Если машины/тулчейна нет — пометить ручную проверку как blocked, не выдавать
  за выполненную.
- Не оставлять сервисы, пайпы, процессы и временные файлы после проверок.

ОГРАНИЧЕНИЯ
- root/polkit/UAC — только по явному действию пользователя.
- Секреты не логировать и не коммитить.
- Не менять формат протокола и macOS-установщик без необходимости.

ОТЧЁТ
- Обнови TASKS.md (S3-T3, S3-T4) и docs/stage-3-report.md (раздел G3.2, ручная
  матрица «проверено / не проверено и почему»).
- NEXT_SESSION.md: состояние и следующий промпт G3.3 (S3-T5, S3-T6).
- Коммиты: feat:/fix:/test: для кода, отдельный docs: для документации.
```

---

## Заметки

- Открытые пункты этапа 3: S3-T3 (Linux), S3-T4 (Windows), S3-T5 (полировка
  установки в UI), S3-T6 (notices/GPL), S3-T7 (smoke на чистых системах),
  S3-T8 (отчёт и handoff этапа 4).
- **S3-T2 не закрыт**: реальная установка LaunchDaemon и `launchctl print` не
  проверены. Это нужно сделать перед S3-T7.
- Известные ограничения G3.1: osascript-скрипт во временном каталоге
  (TOCTOU — для релиза нужен SMJobBless/Authorization Services или подписанный
  установщик); ad-hoc подпись только для dev (R7); `--allow-uid` фиксирует
  одного пользователя; helper не включён в Tauri-бандл (упаковка — S3-T6).
- Реальная подписка и root для TUN не требуются в G3.2; TUN уже покрыт S2-T8.
