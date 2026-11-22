# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, вечер)

- Этапы 0–8 закрыты; Windows-функционал live PASS (research §36), Linux §34.
- **Этап 9 закрыт почти полностью** (research §37/§38): релизный конвейер
  работает, **v0.1.0 опубликована и принята пользователем** — переустановка
  из релиза (новый pubkey) прошла, `update_check` live отвечает, автозапуск
  подтверждён. Остатки: полная цепочка обновления — **тест при v0.1.1**,
  S9-T3 (connect_on_start, автовыбор).
- **S9-T4 done** (`0768e36`): кнопка (i) «Как писать шаблон» в форме
  добавления правила — только для типа «Regex по пути» (самообновляющиеся
  приложения меняют каталог версии: Discord `app-1.0.9269` → `app-1.0.9270`);
  шпаргалка — удвоение `\`, экранирование точки, якоря, `app-[0-9.]+`,
  пример Discord. Строки ru/en, тест (115 frontend).
- **Этап 10 открыт (ближайший план)**: S10-T1 hot-reload per-app правил —
  правки правил применяются к активной сессии без переподключения.
- **v0.1.1 выпущена** (см. ниже): версия поднята в
  tauri.conf.json/package.json/Cargo.toml, тег → release.yml → опубликована.
  Это релиз для полной live-проверки обновления.
- Ключ подписи: `~/.commandcode/splitroute-updater.key` + `.key.pass`,
  GitHub Secrets записаны. main запушен, проверки зелёные (Rust + frontend 115).

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Полная цепочка обновления в приложении (S9-T2 финал) | в установленном приложении (v0.1.0): «Проверить обновления» → доступна 0.1.1 → «Установить» → прогресс → перезапуск → версия 0.1.1 на «Главной»/в настройках. Ядро на момент установки остановлено (before_install), сессию после перезапуска подключить вручную |
| S9-T3 остатки | connect_on_start (перезапуск приложения → «Подключено» без клика), автовыбор локации («Проверить задержки», рекомендация «Лучшая» = мин. пинг); настройки вернуть |
| Regex-правило живым приложением | `process_path_regex` на Windows в рантайме не проверялся (§36 — process_path/process_name): добавить regex-правило Discord по шпаргалке, переподключение, exit-IP целевого приложения |
| S10-T1 hot-reload per-app правил | следующая подцель РЕАЛИЗАЦИИ: правки правил применяются к активной сессии без переподключения (критерий в TASKS.md) |
| blocked извне | S3-T2 (сертификат Apple), PRD-бэклог (IPv6, статичные подписки) |

## Промпт следующей подцели

По умолчанию — сначала короткий live-блок с пользователем (тест обновления
v0.1.0 → v0.1.1 + остатки S9-T3 + regex-правило Discord), затем —
S10-T1 hot-reload.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

ЧАСТЬ 1 — live с пользователем (~15 мин, результат в research §38/TASKS):
  а) Полная цепочка обновления (S9-T2 финал): в установленном приложении
     v0.1.0 «Проверить обновления» → найдена 0.1.1 → «Установить» →
     остановка ядра → прогресс → перезапуск → проверить версию 0.1.1.
     Если апдейтер показывает ошибку подписи — сверить pubkey в
     tauri.conf.json с ~/.commandcode/splitroute-updater.key.pub.
  б) S9-T3 остатки: connect_on_start (включить → перезапуск приложения →
     «Подключено» без клика → выключить), автовыбор («Проверить задержки»,
     «Лучшая» = минимальный пинг → включить автовыбор → выключить).
  в) Regex-правило: добавить правило Discord типа «Regex по пути» по
     подсказке (i), переподключиться, проверить exit-IP целевого приложения.
     Настройки пользователя вернуть как были.
ЧАСТЬ 2 — S10-T1 hot-reload per-app правил (TASKS.md Этап 10): правки
  правил (upsert/delete/toggle) при активной сессии применяются к
  работающему sing-box без разрыва: пересборка routing-части конфига и
  атомарная подмена ядру; ошибки — типизированная ошибка UI с пометкой
  «применится при переподключении»; вне сессии поведение прежнее.
  Изучить: как sing-box принимает изменения (пересоздание инстанса ядра
  с сохранением TUN vs restart), что происходит с kill switch-правилами
  (не рвать их), протокол с helper'ом.
  Проверки: fmt/clippy -D warnings/test, pnpm typecheck/lint/test,
  live-проверка пользователем на Windows.

КОНТЕКСТ WINDOWS (не трогать): установлены приложение (v0.1.1 после части 1)
и helper-служба splitroute-helper-service (Running, auto). Для dev-запуска
закрывать установленное приложение (single-instance). Грабли: MSYS искажает
env с `\\.\pipe\` — dev-app только через cmd-обёртку (sr-gui-launch.cmd в
%TEMP%, туда же PATH на %USERPROFILE%\.cargo\bin); taskkill //F //PID
(двойные слэши); HWID привязан к стору установленного приложения.

ПРОТОКОЛ РАБОТЫ
- Одна подцель за сессию: правки кода + тесты одним коммитом (нативные
  fmt/clippy -D warnings/test, pnpm typecheck/lint/test/build), отдельный
  docs-коммит, NEXT_SESSION.md перезаписать.
- Секреты (ключ подписи, пароль, URL подписки, HWID, UUID, QR) в
  задачи/логи/коммиты не попадают.

ЛИЦЕНЗИИ: при смене крейтов обновлять THIRD_PARTY_NOTICES (тест
packaging_config следит).
```

---

## Заметки

- **Релизный конвейер (research §37):** `release.yml` — тег `v*` → матрица
  windows/ubuntu → `package.sh` с ключом из Secrets → артефакты с `.sig` →
  make-latest-json.sh (`--require-platform`) → draft → upload `--clobber` →
  публикация. Beta-тег `*-beta*` — prerelease + `beta-latest.json`, также
  кладётся на последний стабильный. Тесты в release.yml не повторяются.
  workflow_dispatch принимает input tag для re-run. Грабля: новый `.sh` без
  exec-бита (Write на Windows) → Permission denied в CI; фикс
  `git update-index --chmod=+x`.
- **Updater:** `signature` в манифесте = содержимое `.sig` как есть (v2 CLI
  пишет base64(minisign-текст)); `requireSignedVersion` сверяет версию
  манифеста с `version:` trusted comment; канал — в имени файла
  (`latest.json`/`beta-latest.json`). Debug-override
  `SPLITROUTE_UPDATE_ENDPOINT`.
- **Regex-правила:** каждый `\` пути удваивается, точка экранируется, якоря
  `^…$`, каталог версии — `app-[0-9.]+`; применяется при следующем
  подключении (hot-reload — Этап 10).
- **Windows WFP (research §35):** перечисление по слоям (GUID_NULL =
  FWP_E_LAYER_NOT_FOUND), `FlushFileBuffers` перед `DisconnectNamedPipe`;
  статические WFP-объекты не персистентны — `Restart-Service BFE` снимает
  (аварийный откат, mpssvc поднять следом). Фиксы не откатывать.
- **Linux-рантайм (research §34):** gvisor обязателен (`stack: mixed` ломает
  TCP через TUN с docker — фикс `3810437`); машина `denis@192.168.0.49`
  (fish → `bash -lc`) — не снимать без спроса.
- **S4-T15:** надзор в Rust (`reconnect.rs`), серия до 3 через
  `connect_inner`; `SPLITROUTE_STATE` для изолированного стора, `HOME` не
  переопределять.
- **Kill switch:** протокол 2; после обновления helper'а переустановка
  службы.
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
# ключ подписи updater: ~/.commandcode/splitroute-updater.key (+ .key.pass)
# GitHub Secrets: TAURI_SIGNING_PRIVATE_KEY / ..._PASSWORD — записаны
```
