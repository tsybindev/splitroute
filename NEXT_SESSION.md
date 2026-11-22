# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, поздний вечер)

- **S10-T1 hot-reload реализован** (`274fd17`, bump 0.1.3, research §39):
  per-app правила ушли из статичного конфига в локальный rule-set
  (`rule-apps.json`), sing-box перечитывает его при изменении файла (с 1.10.0,
  watcher fswatch следит за каталогом — rename-подмена безопасна, ошибка
  парсинга оставляет старые правила). Правки правил при активной сессии
  применяются без разрыва: helper (протокол **v3**, команда `reload_routing`)
  атомарно заменяет файл. Kill switch и DNS не затрагиваются. Валидация regex
  до записи в store (`RULES_INVALID`), ошибка применения — `RELOAD_FAILED`
  «сохранено, применится при переподключении». Все проверки зелёные:
  fmt/clippy -D warnings/test (316+39+33+golden 4 c `sing-box check`),
  pnpm typecheck/lint/test (117)/build.
- **Live-блок от пользователя ещё НЕ подтверждён** (жду отчёт):
  1а) обновление 0.1.1→0.1.2 — по косвенным признакам уже прошло (файлы
  Program Files от 22.11 ~14:42, версия exe 0.1.2) — нужно подтверждение:
  шло ли через «Проверить обновления», была ли ошибка лока helper.exe
  (проверка S9-T5); 1б) S9-T3 остатки (connect_on_start, автовыбор);
  1в) regex-правило Discord (первая live-проверка `process_path_regex`).
- main запушен? **НЕТ — коммит `274fd17` ещё локальный** (пуш и релиз v0.1.3
  — после подтверждения live-блока, по решению пользователя).
- Этап 9: релизный конвейер работает, v0.1.2 опубликована (09:51 UTC).
  Ключ подписи `~/.commandcode/splitroute-updater.key` + `.key.pass`,
  GitHub Secrets записаны.

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Отчёт live-блока (S9-T2 финал/S9-T5/S9-T3/regex Discord) | **ожидается от пользователя** — результат в research §38/§39 и TASKS.md; см. список выше |
| Push + релиз v0.1.3 | после подтверждения live-блока: `git push`, тег `v0.1.3` → release.yml → проверка обновления 0.1.2→0.1.3 из приложения (helper получит протокол v3 через хуки установщика); **протокол v3 несовместим с helper v0.1.2 — dev-приложение против установленной службы работать не будет** (dev — через dev-helper на `splitroute-dev`) |
| Live-проверка S10-T1 | включение правила при активном подключении меняет exit-IP целевого приложения без переподключения: Windows (dev-запуск с dev-helper или после релиза), Linux — машина `denis@192.168.0.49` (§34, fish → `bash -lc`) |
| S9-T3 остатки | connect_on_start, автовыбор — из live-блока |
| blocked извне | S3-T2 (сертификат Apple), PRD-бэклог (IPv6, статичные подписки) |

## Промпт следующей подцели

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

ЧАСТЬ 1 — собрать отчёт live-блока (S10-T1 уже реализована, см. §39):
  а) Подтвердить обновление 0.1.1→0.1.2 (шло ли из приложения, ошибка лока
     helper.exe была или нет — закрывает S9-T5) — см. NEXT_SESSION от 22.11.
  б) S9-T3: connect_on_start (перезапуск приложения → «Подключено» без
     клика), автовыбор («Проверить задержки» → «Лучшая»).
  в) Regex-правило Discord по подсказке (i), exit-IP целевого приложения.
     Настройки пользователя вернуть как были.
ЧАСТЬ 2 — push main + тег v0.1.3 → release.yml → подтвердить публикацию
  (beta-latest/latest.json, ассеты с .sig) → пользователю: обновление
  0.1.2→0.1.3 из приложения (ошибки лока быть не должно) → live-проверка
  S10-T1: активная сессия, toggle правила → exit-IP целевого приложения
  меняется без переподключения; kill switch не рвать (многоуровневый откат,
  см. память). Затем live на Linux (машина §34).

КОНТЕКСТ WINDOWS (не трогать): установлены приложение v0.1.2 и
helper-служба splitroute-helper-service (Running, auto, протокол 2).
Грабли: MSYS искажает env с `\\.\pipe\` — dev-app только через cmd-обёртку
(sr-gui-launch.cmd в %TEMP%, туда же PATH на %USERPROFILE%\.cargo\bin);
taskkill //F //PID (двойные слэши); HWID привязан к стору установленного
приложения. Второй splitroute-helper.exe в пользовательской сессии (session 1)
наблюдался 22.11 — если воспроизведётся, разобраться, чей процесс.

ПРОТОКОЛ РАБОТЫ
- Одна подцель за сессию: правки кода + тесты одним коммитом (нативные
  fmt/clippy -D warnings/test, pnpm typecheck/lint/test/build), отдельный
  docs-коммит, NEXT_SESSION.md перезаписать.
- Секреты (ключ подписи, пароль, URL подписки, HWID, UUID, QR) в
  задачи/логи/коммиты не попадают.

ЛИЦЕНЗИИ: при смене крейтов обновлять THIRD_PARTY_NOTICES (тест
packaging_config следит). `regex` уже в инвентаре (идёт через tauri/image).
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
  с ключом из Secrets → артефакты с `.sig` → latest.json. Beta-тег `*-beta*`.
  Грабля: новый `.sh` без exec-бита → `git update-index --chmod=+x`.
- **Updater:** `signature` = содержимое `.sig` как есть; `requireSignedVersion`
  сверяет версию с trusted comment; debug-override
  `SPLITROUTE_UPDATE_ENDPOINT`.
- **Regex-правила:** каждый `\` пути удваивается, точка экранируется, якоря
  `^…$`, каталог версии — `app-[0-9.]+`; теперь применяется к активной
  сессии сразу (S10-T1).
- **Windows WFP (research §35):** статические WFP-объекты не персистентны —
  `Restart-Service BFE` снимает (аварийный откат, mpssvc поднять следом).
  Фиксы не откатывать. Kill switch: протокол 2; после обновления helper'а
  переустановка службы.
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
