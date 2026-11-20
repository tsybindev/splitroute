# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–7: закрыты в своих частях; **S4-T15 (автореконнект ядра) — done
  (2026-11-20, `57158a3`)**, живая проверка пройдена (research §32).
- **Этап 8 (QR-импорт подписки) — код и автопроверки done (2026-11-20,
  `cf385dc`); live-проверка с реальным QR — за пользователем** (шаблон ниже
  и в промпте следующей сессии).
- Windows-часть S4-T5c — функционал проверен пользователем, детальный
  WFP-отчёт ожидается. S3-T2/S3-T7 без изменений (blocked, внешние ресурсы).
- Ветка `main`. CI не менялся.
- Факты: [docs/research.md](./docs/research.md) §33 (QR-импорт: rqrr,
  скоуз «только URL», drag-drop/paste, hermetic-тесты), §32 (автореконнект),
  §31 (kill switch под root), §30 (Clash YAML), §29 (несколько подписок).

### Что работает / проверено

- **QR-импорт (этап 8):** декодирование локально в Rust — `rqrr 0.11`
  (pure Rust, image 0.25 без дублей версий), модуль `src-tauri/src/qr.rs`;
  команды `qr_decode_image` (base64 от вебвью) и `qr_decode_file` (путь от
  drag-drop), обе в `spawn_blocking`. Распознаются `http(s)`-ссылка на
  подписку и Happ-обёртки `happ://add?url=…`/`happ://add/…` (base64
  standard/url-safe, затем percent-decode); успех открывает обычную форму
  добавления с префиллом URL. QR с `vless://` → отдельная ошибка
  (`QR_NODE_LINKS`, импорт узлов без подписки сознательно не поддерживается —
  модель требует URL), мусор → `QR_NO_SUBSCRIPTION`, нечитаемая картинка →
  `QR_DECODE_FAILED`, файл не открылся → `QR_FILE_UNREADABLE`.
- **UI:** диалог `QrImportDialog` — drag-drop через оконное событие
  `onDragDropEvent` (HTML5 dnd при дефолтном `dragDropEnabled` до страницы
  не доходит), выбор файла (`accept` PNG/JPEG), paste скриншота Cmd+V;
  busy-состояние, отмена по токену (запоздавший `FileReader.onload` после
  «Отмены» не открывает форму — регресс-тест). Строки `ru`/`en` парные.
- **Секреты:** декодированный текст QR нигде не логируется; команды не
  пишут URL в лог.
- 443 Rust- и 114 frontend-тестов зелёные; fmt/clippy/typecheck/lint/build,
  `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc` —
  чисто. `THIRD_PARTY_NOTICES` обновлён (+rqrr/g2p/qrcode/zune-*/lru,
  снимок 536 пакетов), `packaging_config` зелёный.
- **Всё из этапов 4–7** — без изменений: несколько подписок,
  автообновление, автоподключение, автореконнект, статистика,
  каталог/иконки, задержки/автовыбор, обновления приложения, локализация,
  helper'ы, kill switch (S4-T5c macOS), Clash YAML.

### Не проверено / осталось

- **Live-проверка QR-импорта с реальным QR** — за пользователем (шаблон —
  промпт следующей сессии). Автопроверки покрывают декодер и UI; живьём не
  проверены: реальный скриншот с экрана, paste в WKWebView (mime может
  отличаться), drag-drop в реальном окне, сквозной импорт → загрузка
  подписки.
- Windows-часть S4-T5c: детальный WFP-отчёт (фильтры `netsh wfp show
  filters`, блокировка `curl.exe` при мёртвом `sing-box.exe`, снятие после
  `stop`, идемпотентность) — записать в TASKS/research, когда будет.
- Kill switch в рантайме **вместе с серией** автореконнекта (правила между
  попытками): покрыто юнит-тестами + S4-T5c; при желании повторить live по
  шаблону research §32 с kill switch на отдельной/подготовленной машине.
- Клик-проверки UI Clash-подписки — Vitest, не автоматизировались.
- Реальный Clash-провайдер с `proxy-providers` (не поддерживается — §30).
- Backlog на будущее: статичные подписки без URL (QR с голыми vless://) —
  требует решения PRD (стора + планировщика); IPv6-маршрутизация (нужно
  решение PRD).
- Blocked (внешние ресурсы): S3-T2 (сертификат Apple), S2-T9/S3-T7 (CachyOS
  GUI/root), S4-T8 (домен манифеста обновлений).

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"`; для live-проверок
  `RUSTUP_HOME`/`CARGO_HOME` задавать явно, если `HOME` переопределяется.
- **`HOME` переопределять для приложения НЕЛЬЗЯ** (research §32): webview
  WKWebView не поднимается — окно белое. Изолированный стор для live —
  **`SPLITROUTE_STATE=/tmp/…/state.json`**.
- **Kill switch + автоподключение при мёртвом туннеле блокирует всю машину**
  (R14, fail-closed) — агентная сессия тоже теряет сеть. Live-проверки
  делать с kill switch выключенным в изолированном сторе.
- **Убить ядро из непривилегированной сессии нельзя** — команда `stop` в
  unix-сокет helper'а (newline-JSON `{"protocol":2,"id":"…","command":"stop"}`),
  для надзора неотличимо от падения. Проба — `/tmp/sr-live-tools/helper_cmd.py`,
  пишется заново и удаляется после проверки.
- **Хвост прошлой live-сессии:** dev-helper ещё запущен под sudo — PID
  82828 (sudo) / 82829, сокет `/tmp/splitroute-dev.sock`, runtime
  `/tmp/splitroute-dev-runtime`, бинарь ядра `/tmp/sr-live-singbox/sing-box`.
  Погасить: `sudo kill 82828 82829` (нужен пароль пользователя). Для
  QR-проверок helper не нужен.
- Live-шаблон (если нужен helper): dev-helper под
  `sudo target/debug/splitroute-helper --socket /tmp/splitroute-dev.sock
  --runtime-dir /tmp/splitroute-dev-runtime --sing-box <abs путь>`; приложение —
  `SPLITROUTE_HELPER_SOCKET=/tmp/splitroute-dev.sock SPLITROUTE_STATE=… pnpm tauri dev`.
- На этой машине в `state.json` — **реальная подписка** пользователя
  (1 запись, 2 профиля) — секреты не читать, не логировать, не коммитить.
  QR с реальным URL пользователь генерирует **локально** (провайдер/свой
  генератор), не через онлайн-сервисы.
- **QR-фича:** зависимости `rqrr 0.11` и dev-`qrcode 0.14`; при добавлении/
  удалении крейтов обновлять снимок в `THIRD_PARTY_NOTICES` (тест
  `packaging_config` следит за packaging-зависимостями). Контракт
  `QrDecodeResult{kind: url|happ|node_links|unknown, url}` — синхронизирован
  в `src-tauri/src/qr.rs` и `src/shared/api/types.ts`. paste в WKWebView
  не проверен живьём.
- Фоновые `&`-процессы не обслуживают TCP; live-сервер — python-оркестратор
  (поток + `subprocess.Popen(start_new_session=True)`), пишется заново в
  `/tmp/sr-*` и удаляется после проверки. Порт фикстуры — свой (18081).
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4,
  reqwest 0.13.5, tokio 1.54.1 (имя воркеров `tokio-rt-worker`).
- Таймаут подписки 45 с не уменьшать; `Accept` подписки не менять.
- После проверок: `pgrep -x sing-box`, `pgrep -fl splitroute-helper`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`; dev-сокет и
  runtime-каталог — `/tmp/splitroute-dev.sock`, `/tmp/splitroute-dev-runtime`.
- Секреты (URL подписки, HWID, UUID, содержимое QR) в задачи, фикстуры, логи
  и коммиты не попадают.

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

Первая часть — закрыть этап 8 живой проверкой QR-импорта. Альтернативы
(если пользователь назовёт их): Windows-WFP-отчёт в документацию, IPv6
(нужно решение PRD).

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: провести live-проверку QR-импорта подписки (этап 8, `cf385dc`) вместе
с пользователем и зафиксировать результаты. ЕСЛИ пользователь назвал другую
задачу — выполняй её. После одной подцели остановись: правки кода (если
найдены баги) + тесты одним коммитом, отдельный docs: коммит, обновление
NEXT_SESSION.md.

ПОДГОТОВКА
- Хвост прошлой сессии: helper под sudo PID 82828/82829 (`sudo kill 82828
  82829`) — для QR он не нужен, но погаси для чистоты, если пользователь
  разрешит.
- Запуск приложения: `SPLITROUTE_STATE=/tmp/sr-qr-state/state.json pnpm tauri dev`
  (каталог создать; HOME не переопределять — белый webview; kill switch в
  изолированном сторе выключен). Helper не нужен — импорт и загрузка
  подписки идут без него.

СЦЕНАРИЙ (просить пользователя, фиксировать результаты)
1. Позитив: пользователь готовит QR со ссылкой на подписку ЛОКАЛЬНО
   (страница/приложение провайдера или свой генератор; URL — секрет, в логи
   и коммиты не попадает). Настройки → Подписки → «Импорт по QR»:
   a) выбрать файл с QR → форма добавления открылась с префиллом URL;
   b) перетащить файл в окно диалога → то же;
   c) скриншот QR (Cmd+Shift+Ctrl+4) → Cmd+V в диалоге → то же (это
      первый живой тест paste в WKWebView — если не сработает, зафиксировать
      mime/поведение в research §33, путь через файл остаётся рабочим);
   d) имя дозаполнить, «Добавить» → подписка добавилась, профили загрузились.
2. Негатив: QR с vless:// → «В QR ссылки на узлы…»; картинка без QR →
   «Не удалось прочитать QR-код»; «Отмена» во время расшифровки ничего не
   открывает.
3. После проверки: pgrep -x sing-box, pgrep -fl splitroute-helper,
   lsof -nP -iTCP:1420 -sTCP:LISTEN, ls -d /tmp/sr-* — чисто.

ДОКУМЕНТАЦИЯ
- research.md §33: раздел «Live-проверка» по результатам (в т.ч. paste
  WKWebView); TASKS.md S8-T3: live-проверка → done со ссылкой на коммит;
  PRD §6.1 — если меняется поведение; NEXT_SESSION.md — перезаписать.

ОТЧЁТ
- Если баги: fix-коммиты с тестами (cargo fmt/clippy/test, pnpm
  typecheck/lint/test/build, cargo check -p splitroute-helper --target
  x86_64-pc-windows-msvc). Документация — отдельный docs: коммит.
```

---

## Заметки

- **Этап 8 (QR, research §33):** rqrr 0.11 — image 0.25 без дублей версий
  (фича `img` по умолчанию); скоуз «только URL» — модель записи требует URL,
  статичные записи — отдельная фича; drag-drop — только через событие Tauri
  (HTML5 dnd не доходит до страницы); happ path-форма берётся до `?`/`#`
  (внутри base64 есть `/`); percent-decode продублирован в `qr.rs` (модуль
  subscription менять нельзя); hermetic-тесты — dev-`qrcode 0.14` без фичи
  image, граница крейтов только PNG-байты; rqrr находит оба кода на
  multi-QR-картинке; при смене крейтов обновлять снимок THIRD_PARTY_NOTICES.
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
