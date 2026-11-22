# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, день)

- **Этап 11 закрыт кодом (`c60742b`, bump 0.1.4, research §40):**
  1) локации показываются в порядке ответа провайдера (убрана
  `sortProfilesByLatency`, «Лучшая» — бейджем); 2) вкл/выкл локаций:
  `SubscriptionState.disabled_profiles` (profile_id стабильный FNV-1a —
  переживает рефетч, prune пропавших), `set_profiles_enabled` с переносом
  выбора (первая включённая этой подписки → любая → `None`), guard'ы
  `select_profile`/`connect_inner` (`PROFILE_DISABLED`), пинг и автовыбор —
  только включённые (фильтр в `profiles_ping_inner`, закрывает и
  `best_profile_id`, и `switch_to`); команда `profiles_set_enabled` при
  Running переподключает туннель на fallback **без отката к выключенной**
  (решение пользователя; при неудаче старта туннель остановлен, ошибка в UI).
  UI: чекбокс в строке, «Включить все»/«Выключить все» per-группу (решение
  пользователя), выключенная строка приглушена, бейдж «Выключена».
  3) **UA-фикс**: дефолтный UA подписки был захардкожен `SplitRoute/0.1.0`
  (стор + fallback fetch.rs, без миграции) при версии 0.1.3. Теперь
  `default_user_agent()` = `SplitRoute/{CARGO_PKG_VERSION}`; при загрузке
  стора пустые и дефолтные прошлых версий переписываются под текущую
  (пользовательские не трогаются — Happ-JSON), форма добавления префиллится
  из `updateStatus.current_version`.
- **Автопроверки**: cargo test --lib 328 (+9), vitest 121 (+5), tsc/lint/
  clippy чисто. Live-проверка в GUI (порядок, чекбоксы, автовыбор,
  обновление 0.1.3→0.1.4 из приложения) — за пользователем после релиза.
- **Релиз v0.1.4 запущен**: main запушен, после зелёного build.yml — тег
  `v0.1.4` → release.yml (§37, ~9 мин по опыту v0.1.3): NSIS/AppImage с
  `.sig`, latest.json, публикация. Проверить публикацию и `latest.json`
  (версия 0.1.4, обе платформы) при следующей сессии, если прогон ещё шёл.
- **Этап 9–10 закрыты live ранее**: hot-reload per-app правил (Windows PASS),
  Linux-проверка S10-T1 отложена пользователем (сценарий §39/TASKS.md).

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Live-проверка v0.1.4 (пользователь) | обновление 0.1.3→0.1.4 из приложения; в GUI: порядок локаций как в подписке, чекбоксы, «Включить/Выключить все», автовыбор не берёт выключенные; UA существующих подписок переписался на `SplitRoute/0.1.4` при первом запуске (если был дефолт 0.1.0) |
| Публикация v0.1.4 | если release.yml ещё шёл: `gh run list --workflow=release`, после успеха `gh release view v0.1.4` (не draft) + latest.json `version:0.1.4` |
| Linux live S10-T1 (отложено пользователем) | AppImage/deb на машине §34 (`denis@192.168.0.49`, fish → `bash -lc`, не снимать без спроса): подключение → правило «Имя процесса» `curl` → toggle → egress меняется без переподключения; платформенный риск — переживание inotify-watcher'ом rename-подмены |
| Next подцель реализации | открыть по TASKS.md/PRD-бэклогу (IPv6-маршрутизация, статичные подписки) или техническому долгу — обсудить с пользователем |
| blocked извне | S3-T2 (сертификат Apple), PRD-бэклог (IPv6, статичные подписки) |

## Промпт следующей подцели

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

Контекст: Этапы 9–11 закрыты кодом; v0.1.4 опубликована (порядок локаций
как в подписке, вкл/выкл локаций, UA следует за версией — research §40).
Live-проверка v0.1.4 в GUI — за пользователем. Linux-проверка S10-T1
отложена (сценарий в research §39 / TASKS.md). Выбери следующую подцель
с пользователем: PRD-бэклог (IPv6-маршрутизация, статичные подписки),
технический долг или новая фича по PRD.

КОНТЕКСТ WINDOWS (не трогать): установлены приложение v0.1.4 (или 0.1.3
до обновления пользователем) и helper-служба splitroute-helper-service
(Running, auto, протокол 3). Для dev-запуска закрывать установленное
приложение (single-instance). Грабли: MSYS искажает env с `\\.\pipe\` —
dev-app только через cmd-обёртку (sr-gui-launch.cmd в %TEMP%, туда же PATH
на %USERPROFILE%\.cargo\bin); taskkill //F //PID (двойные слэши); HWID
привязан к стору установленного приложения.

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

- **Этап 11 механика (research §40):** выключенные — side-список
  `disabled_profiles` в подписке (НЕ поле профиля: рефетч заменяет профили
  целиком; profile_id стабилен — FNV-1a от name+server+port+uuid). Пул
  автовыбора строится в одном месте — `profiles_ping_inner`; фильтр
  `enabled_profiles()` там закрывает и `best_profile_id`, и `switch_to`
  (observe получает уже отфильтрованные results). Переподключение при
  выключении выбранной — в команде `profiles_set_enabled` (не через
  `switch_profile_inner`: его откат к прежней локации тут противопоказан).
- **UA (research §40):** нормализация при загрузке в `read_data` — пустой
  или `SplitRoute/<цифры и точки>` → текущая версия; записывается на диск
  сразу. `SplitRoute/1.2.x` и прочие не-номерные строки считаются
  пользовательскими и сохраняются.
- **S10-T1 механика (research §39):** rule-set `{"version":2,"rules":[...]}`,
  тег `splitroute-apps`, файл `rule-apps.json` пишет helper (tmp+rename,
  0600), watcher sing-box — fswatch по каталогу, parse-error → старые
  правила, ядро живёт. Протокол v3: `StartPayload{config,rule_set,
  kill_switch}` + `reload_routing` + `RELOAD_FAILED`. Грабля serde:
  `rename_all="lowercase"` не разделяет слова — нужен `snake_case`.
- **Релизный конвейер (research §37):** тег `v*` → release.yml → `package.sh`
  с ключом из Secrets → артефакты с `.sig` → latest.json. v0.1.3 прошла
  чисто (~9 мин). Beta-тег `*-beta*`. Грабля: новый `.sh` без exec-бита →
  `git update-index --chmod=+x`.
- **Updater:** `signature` = содержимое `.sig` как есть;
  `requireSignedVersion` сверяет версию с trusted comment; debug-override
  `SPLITROUTE_UPDATE_ENDPOINT`. Ключ подписи `~/.commandcode/splitroute-updater.key`
  (+ `.key.pass`), GitHub Secrets записаны.
- **Regex-правила:** каждый `\` пути удваивается, точка экранируется, якоря
  `^…$`, каталог версии — `app-[0-9.]+`; на ≥0.1.3 применяется к активной
  сессии сразу (S10-T1).
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
