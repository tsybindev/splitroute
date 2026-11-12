# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этап 1 завершён. **Этап 2 завершён**: закрыты G2.1–G2.5 (S2-T1…S2-T8).
- **S2-T10 (DNS-политика) — todo.** Реальный тест обнаружил утечку DNS: на
  macOS 15.8.2 / sing-box 1.14.4 DNS-правила по процессу для TUN не матчатся.
  В UI и логе добавлено предупреждение; поведение конфига пока не менялось.
  Нужно выбрать fallback и обновить PRD §6.4. Это блокер для «0 утечек».
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
- UI: Главная, Локации, Приложения, Настройки, Логи; русские тексты, состояния
  «нет подписки / ошибка / пусто», «Повторить», диагностика без секретов;
  предупреждение о неподтверждённом DNS-разделении.
- Трей и система: меню, single instance, закрытие в трей, autostart-флаг.
- Dev-режим без root: `SPLITROUTE_MOCK_HELPER=1` (debug) → `MockHelperClient`.
- Dev-helper под sudo: сокет передаётся пользователю (`SUDO_UID`/`SUDO_GID`) и
  принимает его uid; `started_at` принимается как int или float.

### Проверено (2026-11-12)

- `pnpm typecheck && pnpm lint && pnpm test && pnpm build` — ok (24 теста).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace` — ok (64 lib + 14 protocol + 4 golden + 1 integration).
- `python3 -m unittest spike/helper/test_helper.py -v` — 9 тестов.
- **Реальная подписка** (URL/HWID через env): 2 профиля VLESS+Reality/TCP,
  0 пропусков; `Happ/4.1.0` обязателен.
- **Реальный TUN** через dev-helper под root: TUN `172.19.0.1`, маршрут через
  `utun`; выбранный `curl` → IP прокси; невыбранный `python3` → домашний IP;
  выключение правила → `curl` снова на домашнем IP; `stop` удаляет TUN и
  маршрут, `sing-box run` завершается.
- **DNS:** remote DoT через прокси работает (mixed-only конфиг), но DNS-правило
  по процессу для TUN не матчится (`dns/tls[remote] is unreferenced`), DNS
  выбранного `dig` = домашний IP. `hijack-dns` + `sniff` не помогли.
- Single instance: второй запуск завершается за 0.2 с, первый остаётся один.
- После теста: ядро остановлено, TUN/маршруты удалены, `state.json` восстановлен,
  временные файлы с реальным JSON/логом удалены.

### Не проверено (причина)

- Трей визуально (иконка, меню, клики) и закрытие окна в трей — нет доступа к
  UI-автоматизации (System Events/screencapture требуют разрешений).
- Chrome/Telegram, сон/пробуждение, смена сети — ручная проверка.
- Регистрация автозапуска — специально не включалась.
- Установка LaunchDaemon/системных сервисов, подпись helper'а — этап 3.
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

## Ручная проверка этапа 2 (осталось)

Реальный TUN/IP/allowlist уже проверены. Осталось:

- [ ] Трей: при включённом «Закрывать окно в трей» закрытие окна оставляет
      приложение живым; меню показывает статус, «Подключить/Отключить»,
      «Открыть SplitRoute», «Выйти».
- [ ] Повторный запуск приложения показывает существующее окно, второй процесс
      не остаётся.
- [ ] Chrome/Telegram: выбрать приложение и убедиться, что его трафик идёт через
      прокси, а невыбранное — напрямую.
- [ ] Сон/пробуждение и смена сети: после пробуждения туннель восстанавливается
      или клиент показывает ошибку и переподключается.
- [ ] Решение по DNS (см. S2-T10): системный DNS для всех или весь DNS через
      прокси. Без решения этап 3 не начинать.

Если helper всё ещё запущен, после проверки остановите его и удалите runtime:

```bash
# Ctrl+C в терминале helper'а, затем
sudo rm -rf /tmp/sr-helper /tmp/sr-helper.sock
lsof -nP -iTCP:1420 -sTCP:LISTEN || echo "1420 свободен"
pgrep -fl 'splitroute|sing-box run' || echo "процессов нет"
```

---

## Промпт следующей подцели: G2.6 (S2-T10, DNS-политика)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G2.6 — S2-T10, выбрать и реализовать DNS-политику после реального
теста. Сначала предложи варианты и дождись решения пользователя; не меняй PRD и
поведение конфига молча. После выбора — минимальная реализация, тесты, коммиты,
обновление docs. Этап 3 не начинай.

КОНТЕКСТ (уже проверено 2026-11-12)
- macOS 15.8.2, sing-box 1.14.4, реальная подписка, dev-helper под root.
- Per-app TUN работает: выбранный curl → IP прокси, невыбранный python3 →
  домашний IP; выключение правила возвращает curl на прямой маршрут; stop
  удаляет TUN и маршруты.
- DNS-проблема: DNS-правила с process_name/process_path для TUN не матчатся,
  `dns/tls[remote] is unreferenced`; DNS выбранного dig уходит в локальный
  resolver (домашний IP). `hijack-dns` по порту 53 и `action: sniff` не помогли.
- Remote DoT через прокси работает: mixed-only конфиг с dns.final=remote
  резолвит и ходит через прокси.
- В UI (Главная, Настройки) и в логе уже есть предупреждение при ProcessSplit
  на macOS; поведение конфига пока не менялось.

ПРОЧИТАЙ СНАЧАЛА
- PRD.md §6.4, §6.7, §10 (R6), §13.
- AGENTS.md §10, §12–§14, §17–§18.
- TASKS.md S2-T10, docs/stage-2-report.md §5, docs/research.md §9.
- src-tauri/src/config/mod.rs, src-tauri/src/commands/mod.rs,
  src/features/{connect,settings}, src/shared/i18n/ru.ts.

ЧТО СДЕЛАТЬ
1. Сравни минимум три варианта и покажи таблицу trade-offs:
   A. Системный DNS для всех (fallback PRD §6.4): убрать remote/DNS-правила,
      сохранить предупреждение; выбранные приложения теряют приватность DNS, но
      поведение предсказуемо.
   B. Весь DNS через прокси: hijack-dns + dns.final=remote; проверить на реальном
      TUN, что и выбранные, и невыбранные приложения резолвят через прокси; учесть
      локальные домены и Private Relay.
   C. Диагностика: попробовать другие матчеры/route-действия (resolve, inbound,
      source) и подтвердить/опровергнуть, что per-process DNS для TUN невозможен.
2. Дождись выбора пользователя. Не меняй PRD молча.
3. После выбора: обнови PRD §6.4, конфиг, golden-тесты, UI/лог и docs/research.md.
   Каждое изменение логики — с тестом.
4. Проверь на реальном TUN, если есть подписка/root: выбранный и невыбранный
   процесс, DNS-ответы, очистка после stop. Секреты — только через env.
5. Коммиты: fix:/feat:/test: для кода, отдельный docs: для документации.
6. Обнови NEXT_SESSION.md: состояние и следующий промпт G3.1.

ПРОВЕРКИ
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- python3 -m unittest spike/helper/test_helper.py -v
- После реального теста: helper остановлен, TUN/маршруты удалены, временные файлы
  с секретами удалены, state.json восстановлен.

ОСТАНОВИСЬ после отчёта и коммитов. Этап 3 не начинай.
```

---

## Промпт этапа 3 после решения DNS: G3.1 (S3-T1, S3-T2)

Использовать только после закрытия S2-T10. Скопируйте текст ниже целиком.

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
