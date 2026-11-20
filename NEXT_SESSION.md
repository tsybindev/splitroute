# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–7: закрыты в своих частях; **S4-T15 (автореконнект ядра) — done
  (2026-11-20, `57158a3`)**, живая проверка пройдена (research §32).
  Windows-часть S4-T5c — функционал проверен пользователем, детальный
  WFP-отчёт ожидается. S3-T2/S3-T7 без изменений (blocked, внешние ресурсы).
- Этап 6 (несколько подписок) — done (2026-11-19, `f824f82`).
- Этап 7 (Clash YAML) — done (2026-11-20, `ceca190`, docs `b50cf60`).
- Ветка `main`. CI не менялся.
- Факты: [docs/research.md](./docs/research.md) §32 (автореконнект: решение,
  live-проверка, грабли HOME и kill switch), §31 (kill switch под root,
  `spawn_blocking`), §30 (Clash YAML), §29 (несколько подписок).

### Что работает / проверено

- **Автореконнект (S4-T15):** детект в Rust-надзоре (`reconnect.rs`, опрос
  раз в 2 с при активной сессии, работает без kill switch и при закрытом
  окне); серия до 3 попыток через общий `connect_inner`, пауза 3 с
  (отменяемая), при исчерпании — `RECONNECT_FAILED` в UI, правила kill
  switch не снимаются, повторная серия до ручного действия запрещена.
  Счётчик сбрасывается успешным стартом и ручным отключением (включая
  переключение локации и stop перед обновлением).
- **Live (2026-11-20):** stop ядра через сокет helper'а → восстановление
  за ~2 с (дважды — счётчик/guard сбрасываются); подменный бинарь → 3
  попытки, исчерпание, красный бокс в UI, ретраев нет; ручное
  «Подключить»/«Отключить» — сброс серии и чистый выход; бейдж «попытка
  N из 3» подтверждён пользователем в реальном окне.
- **Всё из этапов 4, 6 и 7** — без изменений: несколько подписок,
  автообновление, автоподключение, статистика, каталог/иконки,
  задержки/автовыбор, обновления приложения, локализация, helper'ы,
  kill switch (S4-T5c macOS), Clash YAML.
- 423 Rust- и 107 frontend-тестов зелёные; fmt/clippy/typecheck/lint/build,
  `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc` — чисто.

### Не проверено / осталось

- Windows-часть S4-T5c: детальный WFP-отчёт (фильтры `netsh wfp show
  filters`, блокировка `curl.exe` при мёртвом `sing-box.exe`, снятие после
  `stop`, идемпотентность) — записать в TASKS/research, когда будет.
- Kill switch в рантайме **вместе с серией** автореконнекта (правила между
  попытками): покрыто юнит-тестами + S4-T5c; при желании повторить live по
  шаблону research §32 с kill switch на отдельной/подготовленной машине.
- Клик-проверки UI Clash-подписки — Vitest, не автоматизировались.
- Реальный Clash-провайдер с `proxy-providers` (не поддерживается — §30).
- Blocked (внешние ресурсы): S3-T2 (сертификат Apple), S2-T9/S3-T7 (CachyOS
  GUI/root), S4-T8 (домен манифеста обновлений).

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"`; для live-проверок
  `RUSTUP_HOME`/`CARGO_HOME` задавать явно, если `HOME` переопределяется.
- **`HOME` переопределять для приложения НЕЛЬЗЯ** (research §32): webview
  WKWebView не поднимается — окно открывается полностью белым (только
  заголовок), фронтенд при этом исправен. Изолированный стор для live —
  **`SPLITROUTE_STATE=/tmp/…/state.json`** (env-путь стора, образец —
  `SPLITROUTE_HELPER_SOCKET`).
- **Kill switch + автоподключение при мёртвом туннеле блокирует всю машину**
  (R14, fail-closed) — агентная сессия тоже теряет сеть. Live-проверки
  делать с kill switch выключенным в изолированном сторе.
- **Убить ядро из непривилегированной сессии нельзя** (`kill -9` root-процесса
  — operation not permitted; `pkill -x sing-box` не матчится из-за comm 16).
  Рабочий способ — команда `stop` в unix-сокет helper'а (newline-JSON
  `{"protocol":2,"id":"…","command":"stop"}`): для надзора приложения это
  неотличимо от падения. Проба статуса — `/tmp/sr-live-tools/helper_cmd.py`
  (создавался заново, удалён после проверки).
- Live-шаблон: dev-helper под `sudo target/debug/splitroute-helper --socket
  /tmp/splitroute-dev.sock --runtime-dir /tmp/splitroute-dev-runtime
  --sing-box <abs путь>` (sudo требует пароль — команда выполняется
  пользователем); приложение — `SPLITROUTE_HELPER_SOCKET=/tmp/splitroute-dev.sock
  SPLITROUTE_STATE=… pnpm tauri dev`. Подмена бинаря ядра для сценария
  исчерпания: копия sing-box в `/tmp/…` + заглушка `exit 1`.
- На этой машине в `state.json` — **реальная подписка** пользователя
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

По умолчанию — **QR-импорт подписки** (backlog). Альтернативы: Windows-WFP-отчёт
в документацию (когда пользователь предоставит данные), IPv6-маршрутизация
(нужно решение PRD).

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: реализовать QR-импорт подписки из backlog (TASKS.md «Backlog») —
ЕСЛИ пользователь в этой сессии назвал другую задачу, выполняй её. После
одной подцели остановись: коммит кода/тестов, отдельный коммит
документации, обновление NEXT_SESSION.md. Следующие задачи не начинай.

ПРОЧИТАЙ СНАЧАЛА
- TASKS.md: Backlog, строки S7-T1…S7-T3 (Clash YAML — последний формат),
  S4-T10 (base64/vless), S6-T* (несколько подписок).
- PRD.md §6.1 (форматы подписки, User-Agent, HWID), §13 п. 8–9.
- docs/research.md §30 (Clash YAML), §29 (несколько подписок).
- src-tauri/src/subscription/ (детект формата, парсеры), команды
  subscription_add/subscription_fetch в src-tauri/src/commands/mod.rs,
  UI «Настройки» — карточка подписок.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- Helper и протокол не меняются; новые привилегии запрещены.
- Форматы Happ-JSON / base64-vless / Clash YAML продолжают работать;
  детект расширяется, а не переписывается; User-Agent — дискриминатор.
- Секреты (URL, HWID, UUID, содержимое QR) в логи и коммиты не попадают.
- Без новых зависимостей, если декодер QR не потребует иначе (обсудить).
- Blocking-код в командах — только spawn_blocking (research §31).

ПРОВЕРКИ
- cargo fmt/clippy/test --workspace; pnpm typecheck/lint/test/build;
  cargo check -p splitroute-helper --target x86_64-pc-windows-msvc.
- Live-проверка: реальный QR (пользователь показывает/генерирует), загрузка
  из UI; изолированный стор через SPLITROUTE_STATE, kill switch выключен
  (research §32: HOME переопределять нельзя, webview ломается).

ДОКУМЕНТАЦИЯ
- TASKS.md (новая задача → done со ссылкой на коммит), PRD §6.1 (если
  меняется описание форматов), research.md §33 (решение и ограничения),
  NEXT_SESSION.md.

ОТЧЁТ
- Коммиты: feat для кода (тесты вместе с кодом), отдельный docs:.
```

---

## Заметки

- **S4-T15 (research §32):** детект в Rust (`reconnect.rs`), серия через
  `connect_inner`; guard серии снимается при любом выходе (регресс-тест);
  `SPLITROUTE_STATE` для изолированного стора; `HOME` для приложения не
  переопределять — белый webview; stop ядра — через сокет helper'а, а не
  `kill` (root); kill switch + автоподключение при мёртвом туннеле = блэкаут
  машины, live-проверки — с kill switch off.
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
  потом `initialize()`» в App.tsx. События reconnect — там же (S4-T15).
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
