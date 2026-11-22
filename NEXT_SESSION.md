# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, аудит
> реализованного, что осталось и готовый промпт следующей сессии.

## Состояние (2026-11-22, вечер)

- Этапы 0–8 закрыты; **Windows-функционал прогнан live через GUI, блоки 1–5
  PASS** (research §36). Linux-рантайм CachyOS закрыт live 2026-11-21 (§34).
- **Этап 9 реализован (S9-T1 done, S9-T2 done частично)**, research §37:
  `release.yml` (тег `v*` → подписанные артефакты → draft → `latest.json` →
  публикация), `scripts/make-latest-json.sh`, `updates.rs` на реальном
  endpoint GitHub Releases (канал в имени файла: `latest.json` /
  `beta-latest.json`), pubkey заменён. Локально: `package.sh` с ключом
  подписал NSIS, манифест проверен на реальной подписи (trusted comment
  `version:0.1.0`).
- **Keychain: приватный ключ updater перегенерирован** —
  `~/.commandcode/splitroute-updater.key` + `.key.pass` (пароль не печатался),
  GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY`/`..._PASSWORD` записаны через
  `gh secret set` из файлов. **Старый pubkey был с другой машины** —
  установленное приложение 0.1.0 его не знает: после публикации релиза
  приложение ОДИН РАЗ переустановить из релиза.
- **РЕЛИЗ v0.1.0 ОПУБЛИКОВАН**: тег запушен, release.yml run 45005423792 —
  success (первый прогон упал на exec-бите make-latest-json.sh — см. §37).
  https://github.com/tsybindev/splitroute/releases/tag/v0.1.0 — все 8 ассетов
  (NSIS exe+sig, AppImage+sig, deb, rpm, latest.json, beta-latest.json).
  Проверено: `releases/latest/download/latest.json` отдаёт манифест
  (version 0.1.0, signature — base64 minisign, совпадает с `.sig`-ассетом,
  url на ассеты), NSIS-ассет скачивается (200). main запушен (0 не
  отправленных), автопроверки зелёные (Rust + frontend 114).
- Формат signature в latest.json (важно!): содержимое `.sig` **как есть** —
  CLI v2 пишет `.sig` одной строкой base64(minisign-текст), плагин декодирует
  поле и парсит minisign; двойное кодирование даёт UPDATE_SIGNATURE.

## Аудит: что осталось

| Задача | Статус |
|---|---|
| Переустановка приложения из релиза v0.1.0 (новый pubkey) | скачать `SplitRoute_0.1.0_x64-setup.exe` из релиза, закрыть приложение, установить, проверить «Подключено» с реальным helper'ом |
| `update_check` в переустановленном приложении | S9-T2 остаток: в приложении «Проверить обновления» → «У вас последняя версия» (версии равны — сеть/подпись манифеста не задействованы) |
| Полная проверка обновления (скачать → подпись → установка → перезапуск) | при следующем релизе: bump-коммит 0.1.1 (tauri.conf.json/package.json/Cargo.toml), тег, прогон конвейера, обновление приложения по кнопке |
| S9-T3 live-проверки Windows (~30 мин) | чек-лист ниже |
| blocked извне | S3-T2 (сертификат Apple), PRD-бэклог (IPv6, статичные подписки, hot-reload per-app) |

## Промпт следующей подцели

По умолчанию — переустановка из релиза + S9-T3. Вторая по запросу — v0.1.1
для полной проверки обновления.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: C:\Users\Denis\Documents\DEV\SplitRoute\splitroute (Windows, Git Bash).
Перед командами: export PATH="$USERPROFILE/.cargo/bin:$PATH".

ЦЕЛЬ: переустановить приложение из релиза v0.1.0 (новый pubkey updater),
live-проверки S9-T3.

ШАГ 1 — переустановка приложения из релиза (обязательно: старый pubkey):
  закрыть установленное приложение (tasklist | grep -i splitroute),
  скачать SplitRoute_0.1.0_x64-setup.exe из релиза
  (https://github.com/tsybindev/splitroute/releases/tag/v0.1.0), установить
  поверх, запустить: сессия должна подняться с установленным helper'ом
  (splitroute-helper-service), локации/профили пользователя сохранены
  (HWID тот же — стор тот же). Затем «Проверить обновления» → ожидается
  «последняя версия» (S9-T2 остаток).

ШАГ 2 — S9-T3 (live, настройки вернуть как были):
  а) Автозапуск: включить «Запускать при входе в систему» →
     reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
     (запись splitroute с --hidden); выключить → записи нет.
  б) Подключение при старте: включить connect_on_start при сохранённой
     локации → перезапуск приложения → «Подключено» без клика; выключить.
  в) Автовыбор: «Локации» → «Проверить задержки» с реальной подпиской,
     рекомендация «Лучшая» = минимальный пинг; включить автовыбор, отключить.
  Результаты — в docs/research.md (§38), TASKS.md S9-T3 → done.

КОНТЕКСТ WINDOWS (не трогать): установлены приложение
(C:\Program Files\SplitRoute\) и helper-служба splitroute-helper-service
(Running, auto). Для dev-запуска закрывать установленное приложение
(single-instance: dev-копия тихо выходит с кодом 0). Грабли: MSYS искажает
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

- **Релизный конвейер (research §37):** `release.yml` — матрица
  windows-latest/ubuntu-22.04, package.sh с ключом из Secrets (без
  SPLITROUTE_ALLOW_UNSIGNED), артефакты с `.sig`; job публикации:
  make-latest-json.sh (`--require-platform windows-x86_64,linux-x86_64`) →
  `gh release create --draft --generate-notes` → upload `--clobber` →
  `--draft=false`. Beta-тег `*-beta*` — prerelease + beta-latest.json,
  дополнительно кладётся на последний стабильный (prerelease через
  latest/download недостижим). Тесты в release.yml не повторяются (build.yml
  гоняет их на main). workflow_dispatch принимает input tag для re-run.
- **Updater:** `signature` в манифесте = содержимое `.sig` как есть (v2 CLI
  пишет base64(minisign-текст)); `requireSignedVersion` сверяет версию
  манифеста с `version:` trusted comment; канал — в имени файла
  (`latest.json`/`beta-latest.json`). Debug-override `SPLITROUTE_UPDATE_ENDPOINT`.
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
