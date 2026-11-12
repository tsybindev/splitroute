# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этап 1 завершён. **Этап 2 закрыт**: G2.1–G2.6 (S2-T1…S2-T8, S2-T10).
- **S2-T10 (DNS-политика) — done.** Реализованы два режима `proxy` (по
  умолчанию) и `system`; старый `process` читается из `state.json` как `proxy`.
  Конфиг, UI, логи и golden-тесты обновлены, `sing-box check` зелёный.
  Реальный TUN-тест обоих режимов **не проверен** — в окружении сессии нет
  реальной подписки и root (см. ниже).
- S2-T9 (CachyOS) — **blocked**: машины нет, проверка перенесена в этап 3 (S3-T7).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Отчёт этапа 2: [docs/stage-2-report.md](./docs/stage-2-report.md).
- Результаты реальных проверок: [docs/research.md §9](./docs/research.md).

### Что работает

- Подписка: HTTP + rustls, типизированные ошибки, кэш; дефолтный
  `User-Agent: Happ/4.1.0` (иначе провайдер отдаёт base64 вместо JSON).
- Профили: Happ-JSON → `Profile` (VLESS + Reality/TLS + TCP), пропуски с
  причинами; автовыбор первого профиля, сохранение выбранной локации.
- Allowlist: `process_name`, `process_path`, `path_regex`; выключенные правила
  не попадают в конфиг.
- Подключение: конфиг собирается в ядре, helper получает объект конфига;
  connect/disconnect, статус, время сессии.
- **DNS (S2-T10):** в Настройках выбирается `proxy` (весь DNS через прокси,
  remote DoT + local, локальные имена → local, `hijack-dns`) или `system`
  (только local, без перехвата). В логе пишется режим; для `system` — Warn об
  утечке. На Главной бейдж DNS (proxy — success, system — warn).
- UI: Главная, Локации, Приложения, Настройки, Логи; русские тексты, состояния
  «нет подписки / ошибка / пусто», «Повторить», диагностика без секретов.
- Трей и система: меню, single instance, закрытие в трей, autostart-флаг.
- Dev-режим без root: `SPLITROUTE_MOCK_HELPER=1` (debug) → `MockHelperClient`.
- Dev-helper под sudo: сокет передаётся пользователю (`SUDO_UID`/`SUDO_GID`).

### Проверено (2026-11-12)

- `pnpm typecheck && pnpm lint && pnpm test && pnpm build` — ok (24 теста).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace` — ok (66 lib + 16 protocol + 4 golden + 1 integration).
- `python3 -m unittest spike/helper/test_helper.py -v` — 9 тестов.
- `sing-box check` (1.14.4) проходит для golden-конфигов `proxy` и `system`.
- Миграция DNS: `dns_mode: "process"` в `state.json` читается как `proxy`
  (тест `settings_with_legacy_process_migrates_on_load`).
- **Реальная подписка** и **реальный TUN** проверены в G2.5 (S2-T8): выбранный
  `curl` → IP прокси, невыбранный `python3` → домашний IP; `stop` очищает TUN.

### Не проверено (причина)

- **DNS-режимы на реальном TUN (S2-T10)** — в окружении подцели G2.6 нет
  реальной подписки (`SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID` не заданы) и нет
  passwordless root. Подробный чек-лист — ниже.
- Трей визуально (иконка, меню, клики) и закрытие окна в трей — нет доступа к
  UI-автоматизации.
- Chrome/Telegram, сон/пробуждение, смена сети — ручная проверка.
- CachyOS и Windows — нет машин.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- `NODE_ENV=production` в окружении; Vitest задаёт `env: { NODE_ENV: "test" }`.
- TypeScript 6.x (typescript-eslint не поддерживает 7).
- rustls собран с провайдером `ring`; провайдер регистрируется в `run()`.
- Состояние приложения: `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`), права `0600`.
- Unix-сокет helper'а: debug — `$TMPDIR/splitroute-helper.sock`, prod —
  `/var/run/splitroute/helper.sock`; env `SPLITROUTE_HELPER_SOCKET` переопределяет.
- Dev-helper под sudo: `sudo python3 spike/helper/helper.py --socket
  /tmp/sr-helper.sock --runtime-dir /tmp/sr-helper --sing-box
  ./src-tauri/binaries/sing-box-aarch64-apple-darwin`.
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить; передавать
  только через env (`SUBSCRIPTION_URL`, `SUBSCRIPTION_HWID`).
- После работы helper'а: Ctrl+C в его терминале и
  `sudo rm -rf /tmp/sr-helper /tmp/sr-helper.sock` (лог может содержать адреса).

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
```

---

## Ручная проверка (перед стартом следующей подцели)

Сначала UI без root:

- [ ] Настройки → «DNS-режим»: ровно два варианта «Весь DNS через прокси» и
      «Системный DNS», варианта «Раздельный по процессам» нет.
- [ ] При выборе `proxy` видна подсказка про весь DNS через прокси и `.local`;
      при выборе `system` — подсказка/предупреждение об утечке.
- [ ] Главная → карточка DNS: `proxy` — зелёный/success, `system` — warn и
      предупреждение под бейджем.
- [ ] `~/Library/Application Support/splitroute/state.json` с
      `"dns_mode": "process"` (сделайте бэкап) грузится и показывает `proxy`;
      после сохранения настроек в файле уже `"proxy"`.

Реальный TUN (нужны подписка и root — только через env):

- [ ] Запустить dev-helper под sudo (команда выше), приложение — с реальной
      подпиской.
- [ ] Режим `proxy`: в логе при connect есть `DNS: режим proxy — весь DNS через
      прокси`; в логе ядра `dns/tls[remote]` **referenced** (не unreferenced).
- [ ] Режим `proxy`: DNS выбранного приложения идёт через прокси — сравнить
      `dig CH TXT whoami.cloudflare`/`nslookup myip.opendns.com` внутри
      выбранного процесса и вне его; оба должны дать IP прокси.
- [ ] Режим `proxy`: локальные имена (`.local`, `home.arpa`, `localhost`)
      резолвятся локально и не ломаются.
- [ ] Режим `system`: DNS выбранного процесса даёт **домашний** IP; в логе есть
      `Warn` об утечке DNS; предупреждение видно в UI.
- [ ] `stop` удаляет TUN и маршруты, sing-box завершается.

После проверки:

```bash
# Ctrl+C в терминале helper'а, затем
sudo rm -rf /tmp/sr-helper /tmp/sr-helper.sock
lsof -nP -iTCP:1420 -sTCP:LISTEN || echo "1420 свободен"
lsof -nP -iTCP:10808 -sTCP:LISTEN || echo "10808 свободен"
pgrep -fl 'splitroute|sing-box run' || echo "процессов нет"
```

Если что-то не сходится — сначала чинить, потом начинать этап 3.

---

## Промпт следующей подцели: G3.1 (S3-T1, S3-T2)

Использовать только после ручной проверки DNS-режимов выше. Скопируйте текст
ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G3.1 этапа 3 — S3-T1 (Rust-helper) и S3-T2 (macOS LaunchDaemon и
установка/удаление). После подцели остановись: коммит кода, отдельный коммит
документации, обновление NEXT_SESSION.md. Не начинай S3-T3 и дальше.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §8, §10–§14, §17–§18.
- PRD.md §6.8, §9, §10 (R7), §13.
- TASKS.md: S3-T1, S3-T2, этапы 2–3.
- NEXT_SESSION.md (этот файл), docs/stage-2-report.md, docs/research.md §9.
- Текущее состояние: crates/helper (заглушка), crates/protocol (типы IPC),
  src-tauri/src/privileged/{client,installer,mock,mod}.rs, spike/helper/.
- S2-T10 закрыт: DNS-режимы `proxy`/`system` реализованы, трогать их не нужно.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- crates/protocol — единственный источник типов IPC: Request/Response,
  Command::{Install,Uninstall,Start,Stop,Status}, PROTOCOL_VERSION=1,
  MAX_MESSAGE_BYTES=1 МиБ, HelperStatus.
- Протокол: один JSON на строку; запрос {protocol,id,command,payload};
  ответ {protocol,id,ok,payload|error}. Таймаут 10 с.
- Helper принимает только фиксированные команды, не принимает пути от клиента,
  пишет конфиг сам с правами 0600, перед запуском выполняет `sing-box check`,
  не логирует содержимое конфига (UUID/ключи).
- Unix-сокет: права 0600, владелец — пользователь, проверка uid пира
  (SO_PEERCRED на Linux, getpeereid на macOS). Debug — $TMPDIR, prod —
  /var/run/splitroute/helper.sock.
- UI не получает root и не вызывает shell; установка/удаление — только по
  явному действию пользователя с подтверждением.
- Секреты не коммитить и не логировать.

ЗАДАЧИ
1. S3-T1: реализовать Rust-helper в crates/helper (бинарь splitroute-helper)
   по контракту выше; добавить тесты: сериализация/десериализация, неизвестная
   команда, несовпадение версии, лимит сообщения, права сокета, чужой uid
   (если воспроизводимо без root), невалидный конфиг не запускается.
2. S3-T2: macOS LaunchDaemon: plist com.splitroute.helper, бинарь в
   /Library/PrivilegedHelperTools, сокет /var/run/splitroute, установка/удаление
   через HelperInstaller с подтверждением; ad-hoc подпись для dev.
   Реальную установку сервиса выполнять только по явной команде пользователя,
   не в тестах и не автоматически.
3. Проверить, что src-tauri/src/privileged/client.rs совместим с новым helper'ом
   без изменения формата протокола; dev-переключатель SPLITROUTE_MOCK_HELPER=1
   сохранить.
4. Не менять UI-экраны и не начинать S3-T5 (проводка установки в UI) без
   отдельной подцели.

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Ручная проверка на macOS: установка LaunchDaemon, `launchctl print
  system/com.splitroute.helper`, права сокета 0600, start/stop/status;
  после проверки удалить сервис и убедиться, что сокета и процесса нет.
- Не оставлять LaunchDaemon, сокеты, sing-box и временные файлы после проверки.

ОГРАНИЧЕНИЯ
- Реальную подписку и root использовать только с явного разрешения пользователя
  и только через env; TUN-тест не обязателен, если он уже покрыт S2-T8.
- Не устанавливать системные сервисы без явного запроса пользователя.
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.

ОТЧЁТ
- Обнови TASKS.md (S3-T1, S3-T2), создай/обнови docs/stage-3-report.md с ручной
  матрицей «проверено / не проверено и почему».
- NEXT_SESSION.md: состояние, следующий промпт (G3.2 — S3-T3, S3-T4).
- Коммиты: feat:/fix:/test: для кода, отдельный docs: для документации.
```

---

## Заметки

- Следующие подцели этапа 3: G3.2 — Linux systemd/polkit и `.deb`/`.rpm`/AppImage
  (S3-T3), G3.3 — Windows-служба и `wintun.dll` (S3-T4), G3.4 — проводка установки
  в UI и notices (S3-T5, S3-T6), G3.5 — smoke и отчёт (S3-T7, S3-T8).
- Открытые пункты, вынесенные в backlog: `connect_on_start`, автообновление
  подписки по интервалу, проверка задержки локаций.
- Реальная установка helper'а, подпись и TUN — только этап 3.
