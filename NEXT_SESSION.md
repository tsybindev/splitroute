# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–7 закрыты; **S4-T15 (автореконнект) — done (`57158a3`)**; этап 8
  (QR-импорт) — код и автопроверки done (`cf385dc`), live-QR — за пользователем.
- **S2-T9/S3-T7 (Linux-рантайм на CachyOS) — закрыты live 2026-11-21** по
  runbook `docs/linux-runbook.md` (§2–§7). Найден и исправлен баг: TCP через
  TUN на Linux умирал со `stack: mixed` в окружении с docker — **фикс
  `3810437`** (`tun_stack()`: Linux → gvisor; тест на обеих ОС), подтверждён
  end-to-end конфигом приложения. Факты и грабли — **research §34**.
- Ветка `main`, локальные коммиты не запушены (origin отстаёт — push за
  пользователем, если нужен).
- Секретов в задачах/логах/коммитах нет: state.json с реальной подпиской
  удалён с CachyOS в §8, конфиги с сервером не печатались.

### Что работает / проверено (CachyOS, 2026-11-21)

- Сборка и тесты: 427 Rust (1 ignored, разница с macOS — `cfg(target_os)`)
  + 114 frontend; typecheck/lint; sing-box 1.14.4 по sha256.
- Пакеты: `.deb` 34.4 МиБ / `.rpm` 34.4 МиБ / AppImage 137.8 МиБ.
- Headless-smoke helper'а 10/10 (сокет 0600, status/start/stop/restart,
  runtime config.json 0600 + sing-box.log, outbound HTTP 200).
- TUN+per-app+DNS: TUN `tun0` (генератор не задаёт `interface_name`),
  ip-правила 9000–9010/таблица 2022; curl из TUN → прокси, wget → напрямую;
  DNS-hijack systemd-resolved → vless DoT; из GUI Telegram через VPN
  (подтверждено пользователем), удаление правила → домашний IP.
- Kill switch nftables fail-closed (§6b: kill ядра по PID → правила живы,
  egress заблокирован; stop снимает; ALREADY_RUNNING при двойном start).
- GUI: окно не белое (`WEBKIT_DISABLE_DMABUF_RENDERER=1`), статистика,
  каталог, «Проверить задержки» (~15 с — медленно); установка helper'а из
  приложения (pkexec → unit active, сокет `/run/splitroute` достижим uid
  1000, RuntimeDirectoryMode 0711).
- SSH из LAN переживает kill switch — §0 runbook подтверждён живьём.

### Не проверено / осталось

- **Live-проверка QR-импорта с реальным QR** — за пользователем, ~10 минут
  (запуск: `SPLITROUTE_STATE=/tmp/sr-qr-state/state.json pnpm tauri dev`,
  helper не нужен). Сценарий: Настройки → Подписки → «Импорт по QR» → (a)
  файл → форма с префиллом → добавить; (b) drag&drop; (c) скриншот Cmd+V
  (первый живой тест paste в WKWebView — mime зафиксировать в §33);
  негатив: `vless://`-QR → ошибка узлов, не-QR картинка → `QR_DECODE_FAILED`,
  «Отмена» ничего не открывает.
- Установка пакета (`.rpm` через `rpm -i` / распаковка `.deb`) на CachyOS —
  не делалась (runbook §7 п. 6); инспекция `/usr/share/doc/splitroute/`.
- DNS `.local`/mDNS через туннель (Avahi не разворачивался).
- Сон/смена сети на Linux; kill switch вместе с серией автореконнекта
  (юнит-тесты + §6b по отдельности есть).
- Windows-часть S4-T5c: детальный WFP-отчёт — за будущей сессией/пользователем.
- Клик-проверки UI Clash-подписки — Vitest, не автоматизировались.
- Backlog: статичные подписки без URL (QR с голыми vless://), IPv6-
  маршрутизация (решения PRD); hot-reload правил per-app (сейчас — только
  при переподключении; UX-заметка §34).
- Blocked: S3-T2 (сертификат Apple), S4-T8 (домен манифеста обновлений).

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"`; для live-проверок
  `RUSTUP_HOME`/`CARGO_HOME` задавать явно, если `HOME` переопределяется.
- **`HOME` для приложения НЕЛЬЗЯ переопределять** (research §32) — белый
  webview; изолированный стор — `SPLITROUTE_STATE=/tmp/…/state.json`.
- **CachyOS**: `denis@192.168.0.49` (key-auth работает; IP может смениться —
  уточнять), fish — команды через `bash -lc`/`bash -s`. Дерево
  `~/Dev/splitroute` (rsync, origin отстаёт — после push обновлять rsync'ом
  или git pull), target 8.5 ГБ. Системный helper **оставлен установленным**
  (unit active, `--allow-uid 1000`). socat/jq/patchelf поставлены.
- **Скрипты под sudo**: `$HOME=/root` → пути пользователя через
  `getent passwd`; `sudo a; b; c` — sudo только к первому слову; пробы
  протокола — `runuser -u <user> -- socat …` (под root helper отвечает
  `FORBIDDEN «клиент с другим uid»` — это норма peer-credentials).
- **Ядро убивать по PID из `status`**: `pkill -x sing-box` не матчится
  (comm `sing-box-x86_64`); после kill `ps -p` может показывать зомби —
  факт смерти по `pgrep -f 'binaries/sing-box'` (research §34).
- **single-instance**: живой AppImage/установленное приложение мешает
  dev-запуску (exit 143) — закрыть все копии (research §34).
- **Kill switch + автоподключение при мёртвом туннеле блокирует машину**
  (fail-closed) — агентная сессия в LAN выживает, но live-проверки делать
  с kill switch выключенным в изолированном сторе.
- **stack gvisor на Linux** — критично на машинах с docker (§34); не
  откатывать.
- macOS-хвост прошлой сессии: чисто (helper'ов и sing-box нет);
  staging-скрипты `/tmp/sr-linux-staging` удалены.
- Порт фикстур — свой (18081); фоновые `&`-процессы не обслуживают TCP.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box
  1.14.4, reqwest 0.13.5, tokio 1.54.1.
- Таймаут подписки 45 с не уменьшать; `Accept` подписки не менять.
- После проверок: `pgrep -x sing-box`, `pgrep -fl splitroute-helper`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`.
- Секреты (URL подписки, HWID, UUID, содержимое QR) в задачи, фикстуры,
  логи и коммиты не попадают.

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

Кандидаты по приоритету: **live-проверка QR** (за пользователем, 10 минут),
**Windows WFP-отчёт** (нужна машина пользователя), **установка `.rpm` на
CachyOS** (SSH есть: `sudo rpm -i …` + инспекция deb), IPv6/статичные
подписки (нужны решения PRD).

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: выбирается из «Не проверено / осталось» выше. По умолчанию — помочь
пользователю с live-проверкой QR-импорта (чек-лист в этой же секции) или
закрыть Windows WFP-отчёт S4-T5c, если пользователь дал машину.

КОНТЕКСТ LINUX (закрыто 2026-11-21): CachyOS `denis@192.168.0.49` (LAN,
fish — оборачивать в bash), дерево ~/Dev/splitroute, системный helper
установлен. Фикс stack gvisor (`3810437`) не откатывать; грабли sudo/comm/
single-instance — research §34. Машина и helper остаются у пользователя —
не снимать без спроса.

ПРОТОКОЛ РАБОТЫ
- Одна подцель за сессию: правки кода + тесты одним коммитом (fmt/clippy/
  test, pnpm typecheck/lint/test/build, cargo check -p splitroute-helper
  --target x86_64-pc-windows-msvc), отдельный docs: коммит, NEXT_SESSION.md
  перезаписать.
- Секреты (URL подписки, HWID, UUID, QR) в задачи/логи/коммиты не попадают.

ЛИЦЕНЗИИ: при смене крейтов обновлять THIRD_PARTY_NOTICES (тест
packaging_config следит).
```

---

## Заметки

- **Linux-рантайм (research §34):** TCP через TUN умирает со `stack: mixed`
  на Linux в окружении с docker (system-стек) — gvisor обязателен; comm у
  dev-ядра `sing-box-x86_64` (`pkill -x sing-box` не матчится — PID из
  `status`); зомби-окно после kill; exit-IP ≠ entry-IP провайдера;
  ALREADY_RUNNING при двойном start; правила per-app — только при
  переподключении; протокол 2: аргументы команды в `payload`.
- **Этап 8 (QR, research §33):** rqrr 0.11; скоуз «только URL»;
  drag-drop через событие Tauri; hermetic-тесты на dev-`qrcode 0.14`;
  paste в WKWebView не проверен живьём.
- **S4-T15 (research §32):** детект в Rust (`reconnect.rs`), серия через
  `connect_inner`; `SPLITROUTE_STATE` для изолированного стора; `HOME` не
  переопределять; stop ядра — через сокет helper'а.
- **S4-T5c (research §31):** dev-helper под sudo + `SPLITROUTE_HELPER_SOCKET`;
  порядок pf-опций: `hosts` до `user`; статистика первые ~2 с недоступна.
- **Этап 7 (Clash YAML):** порядок веток детекта (JSON → vless → Clash →
  base64); `clash::try_parse` → `Ok(None)` для «не Clash»; `proxy-providers`
  не поддерживаются.
- **Этап 6:** миграция legacy пишет файл сразу; `drafts` вне стора;
  `subscription-updated` — payload одной подписки.
- **S4-T13:** «сначала слушатели, потом `initialize()`» в App.tsx.
- **Kill switch:** протокол 2; после обновления переустановить helper.
- **Локализация:** новые строки — в `ru.ts` и `en.ts` (тест следит за
  парностью и кириллицей).
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
