# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, ночь — финал подцели)

- **S10-T1 hot-reload: реализовано + Windows live PASS** (`274fd17` код,
  bump 0.1.3, research §39): per-app правила в локальном rule-set
  (`rule-apps.json`, тег `splitroute-apps`), sing-box перечитывает его при
  изменении файла (watcher fswatch по каталогу — rename-подмена безопасна;
  parse-error → старые правила, ядро живёт). Протокол **v3**
  (`reload_routing`, `RELOAD_FAILED`); валидация правил до записи
  (`RULES_INVALID`, крейт `regex`); UI: ErrorBox во вкладке правил,
  подсказка S9-T4 обновлена.
- **Windows live подтверждено пользователем**: обновление 0.1.2→0.1.3 из
  приложения без ошибки лока (helper получил протокол v3 через NSIS-хуки);
  toggle правила `curl.exe` при активной сессии меняет exit-IP
  (`api.ipify.org`: прокси ↔ прямой) **без переподключения**; kill switch и
  DNS не тронуты.
- **Этап 9 закрыт полностью live** (вечер): обновление 0.1.1→0.1.2 без лока
  (S9-T5/S9-T2), connect_on_start, автовыбор (S9-T3), regex Discord
  (`process_path_regex` на Windows).
- **v0.1.3 опубликована** (11:12 UTC, не draft): Windows + Linux ассеты с
  `.sig`, `latest.json` корректен. main запушен (`8554fed`), CI зелёный.
- **Linux-проверка S10-T1 отложена** (решение пользователя): SSH с Windows
  к `denis@192.168.0.49` сбрасывается (§34 вёл macOS-оркестратор), sudo за
  пользователем. Сценарий записан в research §39/TASKS.md.

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Linux live S10-T1 (отложено пользователем) | AppImage/deb 0.1.3 на машине §34 (`denis@192.168.0.49`, fish → `bash -lc`, не снимать без спроса): подключение → правило «Имя процесса» `curl` → toggle → egress меняется без переподключения; ключевой платформенный риск — переживание inotify-watcher'ом rename-подмены. Удобнее всего прогоном с macOS-оркестратора как в §34 |
| Next подцель реализации | открыть по TASKS.md/PRD-бэклогу (IPv6-маршрутизация, статичные подписки) или техническому долгу — обсудить с пользователем |
| blocked извне | S3-T2 (сертификат Apple), PRD-бэклог (IPv6, статичные подписки) |

## Промпт следующей подцели

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

Контекст: Этапы 9–10 (Windows) закрыты; S10-T1 hot-reload реализован
(research §39), Windows live PASS, v0.1.3 опубликована. Linux-проверка
S10-T1 отложена пользователем (сценарий в research §39 / TASKS.md).
Выбери следующую подцель с пользователем: PRD-бэклог (IPv6-маршрутизация,
статичные подписки), технический долг или новая фича по PRD.

КОНТЕКСТ WINDOWS (не трогать): установлены приложение v0.1.3 и
helper-служба splitroute-helper-service (Running, auto, протокол 3). Для
dev-запуска закрывать установленное приложение (single-instance). Грабли:
MSYS искажает env с `\\.\pipe\` — dev-app только через cmd-обёртку
(sr-gui-launch.cmd в %TEMP%, туда же PATH на %USERPROFILE%\.cargo\bin);
taskkill //F //PID (двойные слэши); HWID привязан к стору установленного
приложения.

ПРОТОКОЛ РАБОТЫ
- Одна подцель за сессию: правки кода + тесты одним коммитом (нативные
  fmt/clippy -D warnings/test, pnpm typecheck/lint/test/build), отдельный
  docs-коммит, NEXT_SESSION.md перезаписать.
- Секреты (ключ подписи, пароль, URL подписки, HWID, UUID, QR) в
  задачи/логи/коммиты не попадают.
- Kill switch: не рвать соединение при live-тестах, многоуровневый откат.

ЛИЦЕНЗИИ: при смене крейтов обновлять THIRD_PARTY_NOTICES (тест
packaging_config следит).
```

---

## Заметки

- **S10-T1 механика (research §39):** rule-set `{"version":2,"rules":[...]}`,
  тег `splitroute-apps`, файл `rule-apps.json` пишет helper (tmp+rename,
  0600), путь подставляет в пустые `path` (`fill_rule_set_paths`); watcher
  sing-box — fswatch по каталогу, дебаунс 100 мс, parse-error → старые
  правила, ядро живёт. Протокол v3: `StartPayload{config,rule_set,
  kill_switch}` + `reload_routing` + `RELOAD_FAILED`. Грабля serde:
  `rename_all="lowercase"` не разделяет слова — нужен `snake_case`.
  Валидация `rules::validate_rule`: regex-компиляция (крейт `regex`, RE2 ≈
  Go-regexp), лимиты 200/20/1024.
- **Релизный конвейер (research §37):** тег `v*` → release.yml → `package.sh`
  с ключом из Secrets → артефакты с `.sig` → latest.json. v0.1.3 прошла
  чисто (~9 мин). Beta-тег `*-beta*`. Грабля: новый `.sh` без exec-бита →
  `git update-index --chmod=+x`.
- **Updater:** `signature` = содержимое `.sig` как есть;
  `requireSignedVersion` сверяет версию с trusted comment; debug-override
  `SPLITROUTE_UPDATE_ENDPOINT`. Ключ подписи `~/.commandcode/splitroute-updater.key`
  (+ `.key.pass`), GitHub Secrets записаны.
- **Regex-правила:** каждый `\` пути удваивается, точка экранируется, якоря
  `^…$`, каталог версии — `app-[0-9.]+`; на v0.1.3 применяется к активной
  сессии сразу (S10-T1), на ≤0.1.2 — при переподключении.
- **Windows WFP (research §35):** статические WFP-объекты не персистентны —
  `Restart-Service BFE` снимает (аварийный откат, mpssvc поднять следом).
  Фиксы не откатывать. Kill switch: протокол 2→3; после обновления helper'а
  переустановка службы (хуки делают сами).
- **Linux-рантайм (research §34):** gvisor обязателен (`stack: mixed` ломает
  TCP через TUN с docker — фикс `3810437`); машина `denis@192.168.0.49`
  (fish → `bash -lc`) — не снимать без спроса.
- **S4-T15:** надзор в Rust (`reconnect.rs`), серия до 3 через
  `connect_inner`; `SPLITROUTE_STATE` для изолированного стора, `HOME` не
  переопределять.
- **Локализация:** новые строки — в `ru.ts` и `en.ts` (тест следит за
  парностью).
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh`, `fetch-wintun.sh`,
  `build-helper-sidecar.sh`.
- Наблюдался второй `splitroute-helper.exe` в пользовательской сессии
  (session 1) при работающей службе (22.11) — причина не выяснена; если
  повторится, разобраться.

### Проверка перед стартом (Windows)

```bash
export PATH="$USERPROFILE/.cargo/bin:$PATH"
cd splitroute
git status --short && git log --oneline -3 && git log --oneline origin/main..main | wc -l
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
tasklist | grep -i "sing-box" || echo "ядра нет"
tasklist | grep -i "splitroute.exe" || echo "приложение закрыто"
# перед dev-запуском закрыть установленное приложение (single-instance)
# dev-app: env только через cmd-обёртку (MSYS искажает \\.\pipe\ в env):
#   sr-gui-launch.cmd: set SPLITROUTE_HELPER_PIPE=\\.\pipe\splitroute-dev
#                      set SPLITROUTE_STATE=<tmp>
# dev-helper (console, протокол v3): cargo run -p splitroute-helper -- console ...
# ключ подписи updater: ~/.commandcode/splitroute-updater.key (+ .key.pass)
# GitHub Secrets: TAURI_SIGNING_PRIVATE_KEY / ..._PASSWORD — записаны
```
