# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, вечер)

- **Этап 12 закрыт кодом (S12-T1, bump 0.1.6, research §41):** окно
  fail-open при переключении локации закрыто командой `restart` (протокол
  **4**, payload как у `start`). Helper: `HelperCore::restart` — payload
  проверяется **до** остановки старого ядра, живое ядро останавливается без
  снятия правил kill switch, флаг синхронизируется идемпотентным `sync`
  (неизменившийся флаг firewall не дёргает вовсе), затем обычный
  `start_with_files`; на остановленном ядре restart = start. Неудача
  рестарта — паритет с прежним «`stop` + неудачный `start`»: правила сняты,
  ошибка в UI. Приложение: `CoreStartMode::{Fresh,Restart}` +
  `connect_inner_with`; `connect_inner` (кнопка, автоподключение,
  автореконнект) не менялся; `switch_profile_inner` и
  `profiles_set_enabled_inner` больше не зовут `helper.stop()`. Откат
  переключения при неудаче — прежняя локация свежим `start` (ставит правила
  заново); в `set_enabled` отката нет (решение пользователя этапа 11).
  Решение зафиксировано в PRD §13 п. 12.
- **Автопроверки**: fmt/clippy/test зелёные (331 lib на Windows, +3),
  pnpm typecheck/lint/test (122)/build зелёные. Unix-тесты helper'а (5 unit
  со скриптовым «ядром» + 3 интеграционных) не компилируются на Windows —
  **перед тегом v0.1.6 проверить build.yml на ubuntu** (`gh run list
  --workflow=build.yml --limit 1`, урок v0.1.4).
- **Windows-native smoke протокола v4 (без GUI)**: dev-helper console-режим
  (pipe, `--allow-sid`) против реального sing-box 1.14.4, PowerShell-клиент:
  status → start → **restart (новый pid ядра, конфиг подменён)** → status →
  stop — ALL PASS. Kill switch выключен (WFP не трогался). Грабли: PS 5.1
  читает .ps1 без BOM как ANSI — скрипты ASCII; `cloned_ref_to_slice_refs`
  (Rust 1.100) — `&[x.clone()]` → `std::slice::from_ref(&x)` в тестах.
- **Релиз v0.1.6 опубликован** (13:49 UTC, не draft): 8 ассетов (NSIS/
  AppImage + `.sig`, deb, rpm, latest.json, beta-latest.json); build.yml
  зелёный на `19295bd` (4.5 мин — unix-тесты restart прошли), release.yml
  зелёный (8.5 мин); `latest.json`: `version:0.1.6`, подпись с
  `version:0.1.6` в trusted comment. Протокол 4 несовместим с helper'ом
  ≤0.1.5: NSIS-хуки обновляют службу при установке; до этого переключение
  локаций против старого helper вернёт ошибку протокола.
- **Этапы 9–11 закрыты live** (v0.1.5 в проде): hot-reload per-app правил,
  порядок локаций как в подписке, вкл/выкл локаций, UA следует за версией.
  Решения пользователя 22.11: IPv6 — в бэклоге (пока не требуется),
  статичные подписки — отклонены.

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Live-проверка v0.1.6 (пользователь, после релиза) | обновление 0.1.5→0.1.6 из приложения (хуки обновят helper до протокола 4); при включённом kill switch: переключение локации и автовыбор — трафик выбранного приложения не «моргает», счётчик WFP-фильтров (скрипты §35) не падает до 0 в момент переключения |
| Релиз v0.1.6 | **done** — build.yml success на `19295bd`, тег `v0.1.6`, release.yml success, релиз не draft (13:49 UTC), latest.json `version:0.1.6` |
| Linux live S10-T1 (отложено пользователем) | AppImage/deb на машине §34 (`denis@192.168.0.49`, fish → `bash -lc`, не снимать без спроса): подключение → правило «Имя процесса» `curl` → toggle → egress меняется без переподключения; платформенный риск — переживание inotify-watcher'ом rename-подмены |
| Next подцель реализации | PRD-бэклог пуст, кроме отложенного IPv6; техдолг этапа 12 закрыт — обсудить с пользователем (новая фича по PRD, улучшения UX или пауза) |
| blocked извне | S3-T2 (сертификат Apple); IPv6 — в бэклоге по решению пользователя; статичные подписки — отклонены |

## Промпт следующей подцели

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

Контекст: Этапы 9–12 закрыты; v0.1.6 в проде (13:49 UTC 22.11) — команда
`restart`, протокол 4: переключение локации больше не открывает окно
fail-open kill switch (research §41, PRD §13 п. 12). Live-проверка 0.1.6
(обновление 0.1.5→0.1.6, переключение под kill switch) — за пользователем.
Linux-проверка S10-T1 отложена (сценарий в research §39 / TASKS.md).
PRD-бэклог пуст, кроме отложенного IPv6; статичные подписки отклонены.
Выбери следующую подцель с пользователем: новая фича по PRD, улучшение UX
или техдолг (список в TASKS.md/NEXT_SESSION).

КОНТЕКСТ WINDOWS (не трогать): установлены приложение v0.1.6 (после
обновления пользователем; до этого v0.1.5)
и helper-служба splitroute-helper-service (Running, auto, протокол 4 после
обновления, раньше 3). Для dev-запуска закрывать установленное
приложение (single-instance; в сессии 22.11 вечером оно было запущено
пользователем — не прибирать без спроса). Грабли: MSYS искажает env с
`\\.\pipe\` — dev-app только через cmd-обёртку (sr-gui-launch.cmd в %TEMP%,
туда же PATH на %USERPROFILE%\.cargo\bin); taskkill //F //PID (двойные
слэши); HWID привязан к стору установленного приложения; PS 5.1 читает .ps1
без BOM как ANSI (скрипты ASCII). Dev-helper протокола 4:
cargo run -p splitroute-helper -- console --pipe <имя> --runtime-dir <dir>
--sing-box <bin> --allow-sid <SID>; smoke-клиент протокола — PowerShell
NamedPipeClientStream (см. research §41).

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

- **Этап 12 механика (research §41):** идемпотентный `KillSwitch::sync`
  шорт-катится при неизменившемся флаге — рестарт с тем же kill switch
  вообще не дёргает firewall; при смене флага правила применяются до старта
  нового ядра. Неудачный рестарт снимает правила (паритет со старым путём) —
  окно остаётся только на пути неудачи, как и раньше, и закрывается откатом/
  повторным start. `reload_routing` не тронут: правки правил по-прежнему без
  рестарта ядра.
- **Этап 11 механика (research §40):** выключенные — side-список
  `disabled_profiles` в подписке (profile_id стабилен — FNV-1a от
  name+server+port+uuid); пул автовыбора строится в `profiles_ping_inner`
  (фильтр закрывает и `best_profile_id`, и `switch_to`). Переподключение при
  выключении выбранной — теперь через `restart` (S12-T1), откат к
  выключенной запрещён.
- **UA (research §40):** нормализация при загрузке в `read_data` — пустой
  или `SplitRoute/<цифры и точки>` → текущая версия; пользовательские не
  трогаются.
- **S10-T1 механика (research §39):** rule-set `{"version":2,"rules":[...]}`,
  тег `splitroute-apps`, файл `rule-apps.json` пишет helper (tmp+rename,
  0600), watcher sing-box — fswatch по каталогу, parse-error → старые
  правила, ядро живёт. Протокол v3→v4: `StartPayload{config,rule_set,
  kill_switch}` + `reload_routing` + `restart`. Грабля serde:
  `rename_all="lowercase"` не разделяет слова — нужен `snake_case`.
- **Релизный конвейер (research §37):** тег `v*` → release.yml →
  `package.sh` с ключом из Secrets → артефакты с `.sig` → latest.json.
  Грабля: новый `.sh` без exec-бита → `git update-index --chmod=+x`.
- **Updater:** `signature` = содержимое `.sig` как есть;
  `requireSignedVersion` сверяет версию с trusted comment; debug-override
  `SPLITROUTE_UPDATE_ENDPOINT`. Ключ подписи
  `~/.commandcode/splitroute-updater.key` (+ `.key.pass`).
- **Regex-правила:** каждый `\` пути удваивается, точка экранируется, якоря
  `^…$`, каталог версии — `app-[0-9.]+`.
- **Windows WFP (research §35):** статические WFP-объекты не персистентны —
  `Restart-Service BFE` снимает (аварийный откат, mpssvc поднять следом).
  Фиксы не откатывать. Kill switch: протокол 2→3→4; после обновления
  helper'а переустановка службы (хуки делают сами).
- **Linux-рантайм (research §34):** gvisor обязателен (`stack: mixed` ломает
  TCP через TUN с docker — фикс `3810437`); машина `denis@192.168.0.49`
  (fish → `bash -lc`) — не снимать без спроса.
- **S4-T15:** надзор в Rust (`reconnect.rs`), серия до 3 через
  `connect_inner` (Fresh); `SPLITROUTE_STATE` для изолированного стора,
  `HOME` не переопределять.
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
# dev-helper (console, протокол v4): cargo run -p splitroute-helper -- console ...
# smoke протокола v4 без GUI: PowerShell NamedPipeClientStream (research §41)
# ключ подписи updater: ~/.commandcode/splitroute-updater.key (+ .key.pass)
# GitHub Secrets: TAURI_SIGNING_PRIVATE_KEY / ..._PASSWORD — записаны
```
