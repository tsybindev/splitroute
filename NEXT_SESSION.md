# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этап 1 завершён. **Этап 2 завершён**: закрыты G2.1–G2.5 (S2-T1…S2-T8).
- **S2-T10 (DNS-политика) — todo, решение принято.** Реальный тест обнаружил
  утечку: на macOS 15.8.2 / sing-box 1.14.4 DNS-правила по процессу для TUN не
  матчатся. PRD §6.4 обновлён: пользователь выбирает `proxy` (весь DNS через
  прокси, по умолчанию) или `system` (DNS ОС, с предупреждением); `process`
  выводится из UI и мигрирует в `proxy`. В UI/логе уже предупреждение; осталось
  реализовать оба режима. Это блокер для «0 утечек» и этапа 3.
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
- [ ] После реализации S2-T10 проверить оба режима: `proxy` (DNS выбранного и
      невыбранного процесса идёт через прокси, локальные `.local` не ломаются) и
      `system` (DNS ОС, предупреждение об утечке видно).

Если helper всё ещё запущен, после проверки остановите его и удалите runtime:

```bash
# Ctrl+C в терминале helper'а, затем
sudo rm -rf /tmp/sr-helper /tmp/sr-helper.sock
lsof -nP -iTCP:1420 -sTCP:LISTEN || echo "1420 свободен"
pgrep -fl 'splitroute|sing-box run' || echo "процессов нет"
```

---

## Промпт следующей подцели: G2.6 (S2-T10, два DNS-режима)

Решение уже принято в PRD §6.4: пользователь выбирает `proxy` или `system`.
Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G2.6 — S2-T10, реализовать два пользовательских DNS-режима:
`proxy` (весь DNS через прокси, по умолчанию) и `system` (DNS ОС, с
предупреждением об утечке). Старый `process` вывести из UI и мигрировать в
`proxy`. После подцели остановись: тесты, коммит кода, отдельный коммит
документации, обновление NEXT_SESSION.md. Этап 3 не начинай.

КОНТЕКСТ (проверено 2026-11-12)
- macOS 15.8.2, sing-box 1.14.4, реальная подписка, dev-helper под root.
- Per-app TUN работает: выбранный curl → IP прокси, невыбранный python3 →
  домашний IP; выключение правила возвращает curl на прямой маршрут; stop
  удаляет TUN и маршруты.
- Per-process DNS для TUN не работает: dns-правила по process_name/path не
  матчатся, `dns/tls[remote] is unreferenced`, DNS выбранного dig уходит в
  локальный resolver. `hijack-dns` по порту 53 и `action: sniff` это не
  исправляют.
- Remote DoT через прокси работает: mixed-only конфиг с dns.final=remote
  резолвит и ходит через прокси. `{"port":[53],"action":"hijack-dns"}` проходит
  `sing-box check`.
- PRD §6.4 уже обновлён: режимы `proxy`/`system`, `process` выводится.
  В UI/логе пока есть только предупреждение; конфиг ещё не менялся.

ПРОЧИТАЙ СНАЧАЛА
- PRD.md §6.4, §6.6, §6.7, §10 (R6), §13.
- AGENTS.md §10, §12–§14, §17–§18.
- TASKS.md S2-T10, docs/stage-2-report.md §5, docs/research.md §9.
- src-tauri/src/config/mod.rs, src-tauri/src/commands/mod.rs,
  crates/protocol/src/model.rs, src/features/{connect,settings},
  src/shared/i18n/ru.ts, src/shared/api/types.ts.

КОНТРАКТ
- `DnsMode` в crates/protocol: `Proxy` (serde "proxy") и `System` (serde
  "system"). Старый `process` из state.json должен читаться и трактоваться как
  `proxy`; выбери минимальный способ (deprecated-вариант с маппингом или
  кастомный deserialize) и покрой тестом.
- `Settings::default().dns_mode = Proxy`.
- `proxy`: dns.servers = remote (tls 1.1.1.1, detour proxy) + local;
  dns.rules: локальные имена (localhost, .local, home.arpa) → local;
  dns.final = remote; route.rules: первым `{"port":[53],"action":"hijack-dns"}`.
  Никаких process-правил DNS.
- `system`: только local, rules пустые, final local; hijack-dns не добавлять.
- Старый `ProcessSplit` в конфиге больше не должен создавать process-правила;
  он ведёт себя как `proxy`.
- UI: в Настройках только два варианта; подсказки:
  proxy — «весь DNS через прокси, утечек нет; DNS невыбранных приложений тоже
  идёт через прокси, локальные .local могут не работать»;
  system — «DNS ОС, совместимо; DNS выбранных приложений уходит локально».
  На Главной бейдж DNS: proxy — info/success, system — warn + предупреждение.
- Логи: при connect писать выбранный режим; для system — Warn об утечке DNS.
- Секреты (URL, HWID, UUID) не логировать и не коммитить.

ЗАДАЧИ
1. Обнови `DnsMode` и миграцию старого `process`; тесты serde/миграции.
2. Перепиши `build_dns` и `build_route`: режимы proxy/system, локальные имена в
   proxy, hijack-dns только в proxy. Обнови unit- и golden-тесты; прогони
   `sing-box check`.
3. Обнови UI (Настройки, Главная, i18n, типы) и тесты: два варианта, подсказки,
   предупреждение system, отсутствие process.
4. Проверь на реальном TUN, если есть подписка/root (только через env):
   - proxy: DNS выбранного и невыбранного процесса идёт через прокси (сравни
     `dig CH TXT whoami.cloudflare`/`nslookup` с домашним IP; remote должен быть
     referenced), локальные имена не ломаются;
   - system: DNS выбранного процесса = домашний IP, предупреждение видно;
   - stop очищает TUN/маршруты.
   Если реальной подписки/root нет — пометь «не проверено» с причиной.
5. Обнови PRD (если нужно уточнить), TASKS S2-T10, docs/research.md,
   docs/stage-2-report.md, NEXT_SESSION.md.
6. Коммиты: feat:/fix:/test: для кода, отдельный docs: для документации.

ПРОВЕРКИ
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- python3 -m unittest spike/helper/test_helper.py -v
- После реального теста: helper остановлен, TUN/маршруты удалены, временные файлы
  с секретами удалены, state.json восстановлен, порты 1420/10808 свободны.

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
