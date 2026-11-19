# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–4: закрыты в своих частях; **S4-T5c (macOS) — done (2026-11-20)**,
  Windows-часть — функционал проверен пользователем на реальной машине,
  детальный WFP-отчёт ожидается. S3-T2/S3-T7 без изменений.
- Этап 6 (несколько подписок) — done (2026-11-19, `f824f82`).
- Этап 7 (Clash YAML) — done (2026-11-20, `ceca190`, docs `b50cf60`).
- **Рантайм-проверка kill switch macOS (S4-T5c) — done (2026-11-20).**
  Попутно найдены и исправлены 2 бага: `c3aa3ca` (порядок правила pf) и
  `a27161b` (blocking-команды в `spawn_blocking`). Детали — research §31.
- Ветка `main`. CI не менялся.
- Факты: [docs/research.md](./docs/research.md) §31 (kill switch под root,
  грабли pkill и spawn_blocking), §30 (Clash YAML), §29 (несколько подписок).

### Что работает / проверено

- **Kill switch macOS (S4-T5c):** подключение с pf-анкером
  `com.apple/splitroute.killswitch` (правила применяются, pf.conf не тронут);
  прямой egress заблокирован при живом ядре, LAN/loopback живы; реальное
  убийство ядра → `state: stopped`, правила остаются, egress продолжает
  блокироваться (fail-closed), UI показывает `CORE_STOPPED` через ~2 с; после
  отключения — anchor not found, трафик восстановлен, pf Disabled. Откат
  `KILL_SWITCH_FAILED` не оставляет включённый pf.
- **Clash YAML (этап 7):** четвёртый формат подписки — детект в цепочке
  `subscription/mod.rs`, парсер `subscription/clash.rs` (только
  `type: vless` + TCP + Reality/TLS), JSON-нотация и flow-стиль принимаются.
- **Всё из этапов 4 и 6** — без изменений: несколько подписок, автообновление,
  автоподключение, статистика, каталог/иконки, задержки/автовыбор, обновления
  приложения, локализация, helper'ы и установщики.
- 413 Rust- и 103 frontend-теста зелёные; fmt/clippy/typecheck/lint/build,
  `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc` — чисто.

### Не проверено / осталось

- **S4-T15 (todo, задача в TASKS.md): автореконнект ядра** — детект работает,
  автоматического переподключения (до 3 попыток, PRD §13 п. 4) нет.
- **Windows-часть S4-T5c:** пользователь проверил функционал на реальной
  машине; детальный WFP-отчёт (фильтры `netsh wfp show filters`, блокировка
  `curl.exe` при мёртвом `sing-box.exe`, снятие после `stop`, идемпотентность)
  — записать в TASKS/research, когда будет предоставлен.
- Клик-проверки UI Clash-подписки — покрыты Vitest, не автоматизировались.
- Реальный Clash-провайдер с `proxy-providers` (не поддерживается — research §30).
- Blocked (внешние ресурсы): S3-T2 (сертификат Apple), S2-T9/S3-T7 (CachyOS
  GUI/root), S4-T8 (домен манифеста обновлений).

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"`; для live —
  `RUSTUP_HOME`/`CARGO_HOME` **до** переопределения `HOME`.
- **Проверка kill switch без подписи Apple (research §31):** dev-helper под
  `sudo target/debug/splitroute-helper --socket /tmp/splitroute-dev.sock
  --runtime-dir /tmp/splitroute-dev-runtime --sing-box <abs путь к бинарю>`;
  приложение — `SPLITROUTE_HELPER_SOCKET=/tmp/splitroute-dev.sock
  pnpm tauri dev`. Под sudo `SUDO_UID` подхватывается, сокет chown'ится на
  пользователя. Прямой пробой протокола: newline-JSON
  `{"protocol":2,"id":"…","command":"status"}` в unix-сокет.
- **`pkill -x sing-box` НЕ убивает ядро** (comm обрезается до 16 символов,
  `sing-box-aarch64`); правильные варианты: `pkill -x
  sing-box-aarch64-apple-darwin` или `kill -9 <pid из статуса helper'а>`.
  Из-за этого первые попытки сценария краша были ложными.
- **Blocking-код в командах — только через `spawn_blocking`**:
  `#[tauri::command(async)]` на синхронном теле исполняет его на воркере
  tokio, где blocking-reqwest паникует в debug (дроп внутреннего runtime,
  reqwest 0.13 `blocking/wait.rs::enter`). Симптом: промис не разрешается,
  спиннер вечно. Планировщики уже на `spawn_blocking`.
- Порядок опций pf строгий: `hosts` (`all`, `from…to…`) — до `user`/
  `keep state`; существующий тест правил закодировал неверный порядок —
  текст правил синтаксисом не проверялся (research §31).
- На этой машине в `state.json` появилась **реальная подписка** пользователя
  (1 запись, 2 профиля) — секреты не читать, не логировать, не коммитить.
- Фоновые `&`-процессы не обслуживают TCP; live-сервер — python-оркестратор
  (поток + `subprocess.Popen(start_new_session=True)`), пишется заново в
  `/tmp/sr-*` и удаляется после проверки. Порт фикстуры — свой (18081).
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4,
  reqwest 0.13.5, tokio 1.54.1 (имя воркеров `tokio-rt-worker`).
- Таймаут подписки 45 с не уменьшать; `Accept` подписки не менять.
- После проверок: `pgrep -x sing-box`, `pgrep -fl splitroute-helper`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`; dev-сокет и
  runtime-каталог — `/tmp/splitroute-dev.sock`, `/tmp/splitroute-dev-runtime`.
- Секреты (URL подписки, HWID, UUID, адреса) в задачи, фикстуры, логи и
  коммиты не попадают.

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
pgrep -x sing-box || echo "ядра нет"
```

---

## Промпт следующей подцели

По умолчанию — **S4-T15: автореконнект ядра** (прямое продолжение находки
S4-T5c). Альтернативы: QR-импорт (backlog), Windows-WFP-отчёт в документацию.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: реализовать S4-T15 — автореконнект ядра после падения (до 3 попыток),
PRD §13 п. 4 / R13 — ЕСЛИ пользователь в этой сессии назвал другую задачу,
выполняй её. После одной подцели остановись: коммит кода/тестов, отдельный
коммит документации, обновление NEXT_SESSION.md. Следующие задачи не начинай.

ПРОЧИТАЙ СНАЧАЛА
- TASKS.md: строка S4-T15, S4-T5c, S4-T13 (автоподключение), S4-T6 (переключение).
- PRD.md §13 п. 4 (R13), §6.1–6.2.
- docs/research.md §31 (что подтверждено в рантайме, как детектится падение).
- src/shared/state/appStore.ts (`pollConnection`/`protectedCrash`),
  src/features/connect/ConnectScreen.tsx (опрос раз в 2 с),
  src-tauri/src/commands/mod.rs (`connect_inner`, `disconnect_inner`),
  crates/helper/src/core.rs (status/reap, протокол).

ТЕКУЩЕЕ ПОВЕДЕНИЕ
- Детект: фронт опрашивает `helper_status` раз в 2 с при активном kill switch;
  при `state: stopped` на подключении — `CORE_STOPPED` и состояние «error».
  Правила kill switch остаются до отключения (fail-closed, проверено §31).
- Автопереподключения нет: пользователь должен нажать кнопку сам.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- Helper и протокол не меняются; новые привилегии запрещены.
- Fail-closed: между попытками правила kill switch остаются (если были);
  при исчерпании серии — явно сообщить пользователю, правила остаются до
  ручного отключения (не снимать молча).
- Общий путь `connect_inner` (без дублирования логики кнопки), счётчик попыток
  сбрасывается успешным стартом и ручным отключением.
- Не блокировать окно: серии попыток — фоном (`spawn_blocking`), состояние —
  событие/статус для UI; строки `ru`/`en` (тест парности ключей).
- Blocking-код в командах — только `spawn_blocking` (research §31, `a27161b`).
- Секреты в логи не попадают; без новых зависимостей.

ПРОВЕРКИ
- cargo fmt/clippy/test --workspace; pnpm typecheck/lint/test/build;
  cargo check -p splitroute-helper --target x86_64-pc-windows-msvc.
- Live-проверка по шаблону research §31: dev-helper под sudo +
  `SPLITROUTE_HELPER_SOCKET`, реальный профиль; убить ядро (pkill -x
  sing-box-aarch64-apple-darwin или kill по pid из статуса helper'а) —
  убедиться: 3 попытки, паузы, успех → подключение восстановлено и счётчик
  сброшен; при недоступном сервере — серия исчерпана, предупреждение,
  правила kill switch остаются; ручное отключение сбрасывает серию.
  После проверки всё остановить, порты/процессы чисты.

ДОКУМЕНТАЦИЯ
- TASKS.md (S4-T15 → done со ссылкой на коммит), PRD §13 п. 4/R13 (статус
  автореконнекта), research.md §32 (решение и ограничения), NEXT_SESSION.md.

ОТЧЁТ
- Коммиты: feat/test для кода, отдельный docs:. Статусы задач в TASKS.md.
```

---

## Заметки

- **S4-T5c (research §31):** dev-helper под sudo + `SPLITROUTE_HELPER_SOCKET`;
  `pkill -x sing-box` не матчится (comm 16 символов) — только полное имя
  бинаря или PID; blocking-код в командах — только `spawn_blocking`; порядок
  pf-опций: `hosts` до `user`; статистика первые ~2 с после подключения
  недоступна (один WARN — норма).
- **Этап 7 (Clash YAML):** порядок веток детекта важен (JSON → vless → Clash →
  base64; YAML — надмножество JSON); `clash::try_parse` возвращает `Ok(None)`
  для «не Clash»-тел; Reality — по наличию `reality-opts.public-key`;
  `skip-cert-verify: true` — скип; порт строго `as_u64`, SNI только
  `servername`; `proxy-providers` не поддерживаются.
- **Этап 6 (несколько подписок):** миграция legacy пишет файл сразу (id
  случайный); попытка обновления запоминается в `subscription_fetch_inner`;
  guard загрузки — `Mutex<HashSet>` по id; UI-черновики `drafts` живут вне
  стора; событие `subscription-updated` — payload одной подписки.
- **S4-T13:** подписка на `autoconnect-failed` и порядок «сначала слушатели,
  потом `initialize()`» в App.tsx.
- **S4-T5b (Windows):** адреса TUN задублированы в
  `src-tauri/src/config/mod.rs` и `crates/helper/src/killswitch/rules.rs` —
  менять вместе.
- **Kill switch:** протокол 2; после обновления переустановить helper.
- **S4-T4 (статистика):** локальный Clash API; после перезапуска окна — «—»
  до переподключения; первые ~2 с после старта ядра недоступна (норма).
- **S4-T3 (локализация):** новые строки — в `ru.ts` и `en.ts`; тест запрещает
  кириллицу в `en` и проверяет парность ключей.
- **Лицензии:** при смене sing-box/wintun/updater/serde_yaml править
  `THIRD_PARTY_NOTICES`; тест `packaging_config` это проверяет.
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
