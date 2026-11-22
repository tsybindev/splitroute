# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, ночь)

- **S10-T1 hot-reload реализован и опубликован** (`274fd17` код, `deb78fc`/
  `81271da` docs, bump 0.1.3, research §39): per-app правила в локальном
  rule-set (`rule-apps.json`, тег `splitroute-apps`), sing-box перечитывает
  его при изменении файла (watcher fswatch по каталогу — rename-подмена
  безопасна; parse-error → старые правила, ядро живёт). Протокол **v3**
  (`reload_routing`, `RELOAD_FAILED`); валидация правил до записи
  (`RULES_INVALID`, крейт `regex`); UI: ErrorBox во вкладке правил,
  RELOAD_FAILED синхронизирует список, подсказка S9-T4 обновлена.
- **Этап 9 закрыт полностью live** (пользователь, вечер): обновление
  0.1.1→0.1.2 через приложение без ошибки лока helper.exe (S9-T5/S9-T2),
  connect_on_start, автовыбор («Лучшая») — S9-T3 закрыт. Regex-правило
  Discord работает — **`process_path_regex` проверен на Windows** (на
  v0.1.2 применение при переподключении).
- **v0.1.3 опубликована** (2026-11-22 11:12 UTC, не draft): ассеты Windows
  (NSIS + `.sig`) и Linux (deb/rpm/AppImage + `.sig`), `latest.json` версии
  0.1.3 с корректной подписью. main запушен, проверки зелёные (build workflow
  + release workflow success).
- Осталась **live-проверка S10-T1** (см. ниже) и Linux-прогон.

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Обновление 0.1.2→0.1.3 из приложения | пользователь: «Проверить обновления» → 0.1.3 → «Установить» (ошибки лока быть не должно) → перезапуск → версия 0.1.3. Helper-служба после обновления — протокол v3 (NSIS-хуки перезапускают). Сессию после перезапуска подключить вручную |
| Live-проверка S10-T1 (критерий приёмки) | активная сессия, toggle правила → exit-IP целевого приложения меняется **без переподключения**. Точный сценарий: правило «Имя процесса» `curl.exe` включить → `curl.exe https://api.ipify.org` в терминале → exit-IP прокси; выключить правило → curl снова → прямой IP. Между toggle переподключения НЕТ. Kill switch и DNS-режим не должны измениться (сессия живёт) |
| Live на Linux (машина §34) | `denis@192.168.0.49` (fish → `bash -lc`), не снимать без спроса: обновить до 0.1.3 (deb/AppImage), тот же сценарий toggle+curl, gvisor-стек (§34) |
| S9 остатки | нет — этап закрыт |
| blocked извне | S3-T2 (сертификат Apple), PRD-бэклог (IPv6, статичные подписки) |

## Промпт следующей подцели

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

ЧАСТЬ 1 — live-проверка S10-T1 на Windows (~10 мин, результат в research §39/TASKS):
  а) В приложении 0.1.2 «Проверить обновления» → 0.1.3 → «Установить» →
     перезапуск → версия 0.1.3. Ошибки «Error opening file for writing:
     splitroute-helper.exe» быть не должно. Служба после обновления —
     протокол v3; сессию подключить вручную.
  б) Критерий hot-reload: подключить сессию → добавить правило «Имя
     процесса» = curl.exe → В ТЕРМИНАЛЕ `curl.exe https://api.ipify.org`
     → IP прокси. Выключить правило (toggle) → curl снова → прямой IP.
     Между toggle переподключения НЕТ; статус остаётся «Подключено».
     Включить правило обратно → curl → IP прокси.
  в) Убедиться, что kill switch не тронут (сессия живёт, правила WFP на
     месте) и DNS-режим не изменился. Удалить тестовое правило curl.exe.
ЧАСТЬ 2 — live на Linux (машина denis@192.168.0.49, fish → bash -lc, НЕ
  снимать без спроса): обновить до 0.1.3 (deb или AppImage), тот же сценарий
  toggle+curl, убедиться в gvisor-стеке (research §34). Зафиксировать в
  TASKS.md (S10-T1 done live) и research §39.

КОНТЕКСТ WINDOWS (не трогать): установлены приложение (после части 1 —
v0.1.3) и helper-служба splitroute-helper-service (Running, auto, протокол
3). Для dev-запуска закрывать установленное приложение (single-instance).
Грабли: MSYS искажает env с `\\.\pipe\` — dev-app только через cmd-обёртку
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
