# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–7 закрыты; S4-T15 (автореконнект) — done; этап 8 (QR-импорт) — код
  и автопроверки done, live-QR — за пользователем.
- **S2-T9/S3-T7 (Linux-рантайм CachyOS) — закрыты live 2026-11-21** (research §34).
- **S4-T5c Windows — закрыт live 2026-11-22** (research §35): WFP kill switch
  проверен под управлением агента на Windows 11, **ALL PASS 21/21**. Найдены
  и исправлены два бага helper'а:
  1. перечисление WFP с `layerKey=GUID_NULL` всегда падало
     `FWP_E_LAYER_NOT_FOUND` (0x80320004) — фикс: перечисление по слоям
     ALE_AUTH_CONNECT_V4/V6 (`delete_objects`), регресс-тест
     `crates/helper/tests/wfp_live.rs` (`#[ignore]`, нужен elevation);
  2. гонка named pipe — `DisconnectNamedPipe` выбрасывал незачитанный ответ
     («пустой ответ») — фикс: `FlushFileBuffers` перед disconnect (`pipe.rs`).
  Попутно: latent-линт в `catalog/windows.rs` — Windows-only код впервые
  прошёл нативный clippy.
- Ветка `main`, локальные коммиты не запушены (push — за пользователем).

### Что работает / проверено (Windows 11 10.0.26100, 2026-11-22)

- Сборка/тесты нативно: 393 Rust (2 ignored: macOS/Linux-разница +
  `wfp_live`), 114 frontend; fmt/clippy -D warnings/typecheck/lint/build.
- Пакеты: sing-box 1.14.4 + wintun 0.14.1 (sha256), helper-сайдкар
  `binaries/splitroute-helper-x86_64-pc-windows-msvc.exe`.
- WFP kill switch: `start` c `kill_switch:true` → 18 объектов WFP
  (17 фильтров + подслой + провайдер); loopback/LAN живы, обычный egress
  **refused** (WFP-блок мгновенный, не таймаут!), трафик через ядро → 204;
  kill ядра → fail-closed (правила остаются); `stop` → всё снято до нуля.
- Скрипты проверки и отката: `scripts/windows/wfp-report.ps1` +
  `wfp-watchdog.ps1` (дедлайн 240 с → stop по пайпу → kill helper по
  PID-файлу → `Restart-Service BFE`). Отчёт — `%TEMP%\sr-wfp\report.md`.
- GUI smoke: `pnpm tauri dev` (MockHelper, изолированный стор) — окно
  открывается, webview грузит UI.

### Не проверено / осталось

- Windows kill switch с реальным профилем через GUI + TUN-трафик (в отчёте —
  loopback-инбаунд вместо туннеля); поведение при падении helper'а-службы.
- **Live-QR** (~10 мин, за пользователем): `SPLITROUTE_STATE=<путь>
  pnpm tauri dev`, Настройки → Подписки → «Импорт по QR»; на Windows —
  WebView2 (не WKWebView): mime paste зафиксировать в §33. Негатив:
  `vless://`-QR → ошибка узлов, не-QR → `QR_DECODE_FAILED`.
- Установка пакета `.rpm`/`.deb` на CachyOS (runbook §7 п. 6) — через SSH.
- DNS `.local`/mDNS; сон/смена сети; kill switch + серия автореконнекта
  вместе.
- Клик-проверки UI Clash-подписки (Vitest есть, live — нет).
- Backlog: статичные подписки без URL, IPv6-маршрутизация (решения PRD),
  hot-reload правил per-app.
- Blocked: S3-T2 (сертификат Apple), S4-T8 (домен манифеста обновлений).

### Важные факты окружения

**Windows-машина (эта сессия; Deno — пользователь Denis):**
- Rust rustup 1.29.3 (stable-x86_64-pc-windows-msvc), pnpm 11.24.0
  (`npm i -g pnpm`; corepack в Node 26 нет), MSVC Build Tools 2022,
  Node 26.13.1.
- **Установлены приложение** (`C:\Program Files\SplitRoute\`) **и helper-служба**
  `splitroute-helper-service` (Running, auto) — их не снимать. Для dev-запуска
  **закрывать установленное приложение** (single-instance: dev-копия тихо
  выходит с кодом 0, окна нет — research §34/§35).
- Pipe helper'а: `\\.\pipe\splitroute-helper`, переопределение —
  `SPLITROUTE_HELPER_PIPE`. Dev-helper: `console --pipe <имя> --runtime-dir
  <каталог> --sing-box <путь> --allow-sid <SID>` (SID —
  `[WindowsIdentity]::GetCurrent().User.Value`), только elevated.
- Пробный конфиг ядра без TUN (для WFP-проверок): `mixed` inbound на
  `127.0.0.1:18999` — см. скрипт отчёта.
- Git Bash грабли: `taskkill //F //PID` (двойные слэши); `whoami` —
  GNU-версия, SID брать в PowerShell; вывод `Out-File` — UTF-16, читать
  `Get-Content`.
- WFP: `netsh wfp show filters file=<path>` (XML, grep по GUID
  `6a4c2f9e-1b7d-4c3e-9f2a-5d8b7e6c1a20`); `Restart-Service BFE` снимает все
  не персистентные объекты (аварийный откат; mpssvc поднять следом).
- Cleanup-лист: `tasklist | grep -i "sing-box\|splitroute"`,
  `netstat -ano | grep ":1420"`, `netsh wfp show providers` (0 объектов
  SplitRoute), `%TEMP%\sr-wfp` (raw-отчёты; можно удалять).

**CachyOS (см. §34):** `denis@192.168.0.49` (fish → `bash -lc`), дерево
`~/Dev/splitroute` (rsync — origin отстаёт), системный helper установлен.
`HOME` приложения не переопределять; изолированный стор — `SPLITROUTE_STATE`.
Kill switch + автоподключение при мёртвом туннеле блокирует машину (LAN
выживает).

### Проверка перед стартом (Windows)

```bash
export PATH="$USERPROFILE/.cargo/bin:$PATH"
cd splitroute
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
tasklist | grep -i "sing-box" || echo "ядра нет"
tasklist | grep -i "splitroute.exe" || echo "приложение закрыто"
# перед dev-запуском закрыть установленное приложение (single-instance)
```

WFP live-отчёт (elevated, UAC, ~90 с с кратковременной блокировкой сети):
```powershell
Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','scripts\windows\wfp-report.ps1'
```

---

## GUI-функционал Windows — прогнан (2026-11-22, research §36)

Весь функционал закрыт live через Computer Use. Итог: **блоки 1–4 PASS**
(подписки/QR 6/7 — drag&drop не автоматизируется, остался пользователю;
подключение+per-app с двумя разными exit-IP; kill switch fail-closed
BLOCKED на T+0.5; автореконнект ~2 с + исчерпание + сброс). Найден и
исправлен регресс `subscription_add/update` (Tauri camelCase vs snake_case,
коммит `1c8ae0e`). Остатки:

1. **drag&drop QR** — 10 с вручную: перетащить PNG в диалог «Импорт по QR».
2. **Блок 5 (по явному «да» пользователя)**: обновить службу
   `splitroute-helper-service` фиксированным бинарем (записать исходный
   binPath для отката: `sc qc splitroute-helper-service` → binaryPathName),
   затем kill switch в установленном приложении. Без этого установленная
   копия работает на СТАРОМ helper'е с багами из §35 (KILL_SWITCH_FAILED).
3. Установленное приложение было закрыто для dev-запусков — открыть обратно.

### Проверка перед стартом (Windows)

```bash
export PATH="$USERPROFILE/.cargo/bin:$PATH"
cd splitroute
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
tasklist | grep -i "sing-box" || echo "ядра нет"
tasklist | grep -i "splitroute.exe" || echo "приложение закрыто"
# dev-запуск: закрыть установленное приложение (single-instance)
# dev-app: env только через cmd-обёртку (MSYS искажает \\.\pipe\ в env):
#   sr-gui-launch.cmd: set SPLITROUTE_HELPER_PIPE=\\.\pipe\splitroute-dev
#                      set SPLITROUTE_STATE=<tmp>
```

WFP live-отчёт (elevated, UAC, ~90 с с кратковременной блокировкой сети):
```powershell
Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','scripts\windows\wfp-report.ps1'
```

---

## Промпт следующей подцели

Кандидаты по приоритету: **блок 5 Windows** (обновление helper-службы
установленной копии — нужен явный «да»), **drag&drop QR** (10 с, вручную),
**установка `.rpm` на CachyOS** (SSH: `sudo rpm -i` + инспекция deb),
IPv6/статичные подписки (нужны решения PRD). Linux/macOS — в бэклоге до
закрытия Windows.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: <путь к репо> (Windows: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH" (Git Bash).

ЦЕЛЬ: выбирается из «Не проверено / осталось» выше. По умолчанию — помочь
пользователю с live-проверкой QR-импорта (чек-лист выше; на Windows —
WebView2, mime paste зафиксировать в §33) или Windows kill switch через GUI
+ TUN (скрипты в scripts/windows/, протокол helper'а уже закрыт).

КОНТЕКСТ WINDOWS (закрыто 2026-11-22, research §35): WFP kill switch
проверен live, фиксы не откатывать: перечисление по слоям (layerKey GUID_NULL
падает FWP_E_LAYER_NOT_FOUND) и FlushFileBuffers перед DisconnectNamedPipe
(гонка «пустого ответа»). Установленные приложение и helper-служба на машине
не трогать; для dev закрывать установленное приложение (single-instance).
Пробный конфиг ядра — mixed loopback-инбаунд, не TUN.

КОНТЕКСТ LINUX (закрыто 2026-11-21): CachyOS `denis@192.168.0.49` (LAN,
fish — оборачивать в bash), дерево ~/Dev/splitroute, системный helper
установлен. Фикс stack gvisor (`3810437`) не откатывать; грабли sudo/comm/
single-instance — research §34. Машина и helper остаются у пользователя —
не снимать без спроса.

ПРОТОКОЛ РАБОТЫ
- Одна подцель за сессию: правки кода + тесты одним коммитом (на Windows
  обязательны нативные fmt/clippy -D warnings/test, pnpm typecheck/lint/
  test/build; Windows-only код больше не проверяется кросс-таргетом),
  отдельный docs: коммит, NEXT_SESSION.md перезаписать.
- Секреты (URL подписки, HWID, UUID, QR) в задачи/логи/коммиты не попадают.

ЛИЦЕНЗИИ: при смене крейтов обновлять THIRD_PARTY_NOTICES (тест
packaging_config следит).
```

---

## Заметки

- **Windows WFP (research §35):** `FwpmFilterCreateEnumHandle0` требует
  конкретный layerKey (GUID_NULL = `FWP_E_LAYER_NOT_FOUND`, не wildcard);
  пустой результат на валидном слое — ОК с нулём; WFP-блок = мгновенный
  refused (не таймаут, как pf); `FlushFileBuffers` перед
  `DisconnectNamedPipe` обязателен; статические WFP-объекты не персистентны
  — `Restart-Service BFE`/перезагрузка их снимает.
- **Linux-рантайм (research §34):** TCP через TUN умирает со `stack: mixed`
  на Linux с docker — gvisor обязателен; comm у dev-ядра `sing-box-x86_64`;
  зомби-окно после kill; ALREADY_RUNNING при двойном start; правила per-app
  — только при переподключении; протокол 2: аргументы команды в `payload`.
- **Этап 8 (QR, research §33):** rqrr 0.11; скоуз «только URL»; drag-drop
  через событие Tauri; hermetic-тесты на dev-`qrcode 0.14`; paste в
  WKWebView не проверен живьём; WebView2 — новый кейс.
- **S4-T15 (research §32):** детект в Rust (`reconnect.rs`), серия через
  `connect_inner`; `SPLITROUTE_STATE` для изолированного стора; `HOME` не
  переопределять; stop ядра — через сокет helper'а.
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
  `tauri build` — `fetch-core.sh`, `fetch-wintun.sh` и
  `build-helper-sidecar.sh`.
