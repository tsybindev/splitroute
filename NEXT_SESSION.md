# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS GUI/root).
- **Этап 3:** S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично**
  (Linux-пакеты собраны, установка/`systemctl`/TUN/GUI не проверены; Windows VM нет).
- **Этап 4:** S4-T1…S4-T8, S4-T9, S4-T10 — done.
  **S4-T5c — blocked** (ручная проверка под root и Windows VM).
  **S4-T7 — todo, ждёт решения PRD §13.2**. **S4-T11 — следующая** (запущенные
  процессы).
- Дата проверки: **2026-11-14**. Ветка `main`. Последний коммит кода — `5172dd5`
  (feat: автообновление приложения через Tauri updater). Документация —
  отдельный коммит после него.
- Факты: [docs/research.md](./docs/research.md) §19 (статистика), §20 (kill
  switch), §21 (задержки и автовыбор), §22 (автообновление).

### Что работает

- **Подключение.** UI → `connect` → `build_config` → helper `start`; helper
  запускает sing-box sidecar и принимает только `status/start/stop`.
  **Протокол helper'а — версия 2.** `install`/`uninstall` по IPC запрещены.
  Установленный helper v1 несовместим: нужна переустановка из настроек.
- **Задержки локаций и автовыбор (S4-T6, коммит `2405d09`).**
  - Замер — TCP-connect до `server:port` в непривилегированном приложении:
    ядро не запускается, helper не участвует, UUID/Reality-ключи не читаются.
    3 с на одно TCP-подключение, лимит 64 профиля, 8 параллельных замеров,
    `#[tauri::command(async)]`. Ошибки — `PING_TIMEOUT`, `PING_UNREACHABLE`,
    `PING_DNS`.
  - `profiles_ping` возвращает `results[]` (id, мс/ошибка), `best_profile_id`
    (минимум, при равенстве — первый в подписке) и `switch_to` — рекомендацию
    автопереключения. В логи попадают только имена локаций и числа.
  - Настройки `preferences.auto_select`: `enabled`, `auto_switch`,
    `degradation_ms` (100), `interval_secs` (60), `confirmations` (2),
    `failures_to_switch` (2). По умолчанию всё выключено, старые `state.json`
    читаются через `#[serde(default)]`, диапазоны нормализуются при сохранении.
  - Решение о переключении — в Rust: гистерезис (улучшение не меньше порога),
    несколько подтверждений подряд и серия неудачных замеров текущей локации.
    UI только опрашивает `profiles_ping` по интервалу и вызывает
    `profile_switch`.
  - `profile_switch`: `stop` → выбор → `start` с тем же флагом kill switch.
    Без туннеля только сохраняет выбор. При ошибке старта возвращает прежнюю
    локацию, чтобы не оставить сеть без туннеля и правил.
  - UI: кнопка «Проверить задержки», мс/ошибка, сортировка по задержке и
    «Лучшая» в «Локациях»; карточка автовыбора на «Главной»; группа настроек с
    подсказками; строки `ru`/`en`.
- **Kill switch (S4-T5a, opt-in fail-closed).**
  - `Settings.network.kill_switch` (default `false`, старые `state.json` → `false`).
  - Протокол 2: `StartPayload {config, kill_switch}`, `HelperStatus.kill_switch
    {enabled, error?}`, `ErrorCode::KILL_SWITCH_FAILED`.
  - Helper ставит правила **до** запуска ядра. При падении ядра правила остаются
    (fail-closed); снимаются при `stop`, выходе приложения (`RunEvent::Exit` →
    `stop`) или выключении настройки. Старт helper'а очищает «залипшие» правила
    после захвата сокета. Неудачный старт снимает только правила, поставленные
    этим вызовом.
  - Правила разрешают loopback, локальные сети (`10/8`, `172.16/12`,
    `192.168/16`, `169.254/16`, multicast, IPv6 link-local/ULA/multicast),
    трафик ядра и (на Windows) трафик TUN и DHCP, остальное блокируют. macOS —
    pf anchor `com.apple/splitroute.killswitch` (`/etc/pf.conf` уже содержит
    `anchor "com.apple/*"`; `pfctl -E` + токен в runtime-каталоге `0600`),
    Linux — таблица nftables `inet splitroute_killswitch`, Windows — провайдер
    и подслой WFP (`crates/helper/src/killswitch/windows.rs`).
  - **Windows (S4-T5b, коммит `8c67018`).** WFP, а не `netsh advfirewall`:
    брандмауэр Windows отдаёт приоритет блокирующим правилам, allow-list без
    WFP-подслоя не выражается. Сессия не dynamic — правила переживают падение
    helper'а; при старте helper'а удаляются. `enable`/`disable` идемпотентны:
    в транзакции удаляются фильтры по провайдеру, затем подслой и провайдер.
    Разрешены loopback, локальные сети, DHCP `68→67`, локальный адрес TUN
    (`172.19.0.1`/`fdfe:dcba:9876::1`) и AppId `sing-box.exe`; блок на обоих
    слоях `ALE_AUTH_CONNECT` с весом ниже разрешений. `core.rs` передаёт путь к
    ядру в `KillSwitch::platform`; протокол не менялся.
- **Автообновление (S4-T8, коммит `5172dd5`).**
  - Rust-only `tauri-plugin-updater` 2.14.1; JS-плагин и process-плагин не
    нужны: команды `update_status`/`update_check`/`update_install`, прогресс —
    событие `update-progress`, перезапуск — `AppHandle::restart`.
  - Каналы `stable`/`beta`, манифест `https://updates.splitroute.invalid/<channel>/latest.json`
    — **заглушка** до решения PRD §13.3. `UPDATE_NOT_CONFIGURED` возвращается без
    сетевого запроса. В debug можно задать `SPLITROUTE_UPDATE_ENDPOINT` (HTTPS).
  - Проверка при запуске — opt-in (`preferences.updates.check_on_start`);
    установка только по кнопке: проверка → остановка ядра → загрузка с проверкой
    подписи → перезапуск. Ошибка проверки не рвёт туннель.
  - Подпись: приватный ключ `~/.commandcode/splitroute-updater.key` (+ пароль
    `.key.pass`, права `0600`, в git не попадают), публичный — в
    `src-tauri/tauri.conf.json`; `requireSignedVersion: true`. `package.sh`
    требует `TAURI_SIGNING_PRIVATE_KEY`/`..._PASSWORD` и создаёт
    `createUpdaterArtifacts`; `SPLITROUTE_ALLOW_UNSIGNED=1` — сборка без updater.
  - UI: карточка «Обновления» в настройках — версия, канал, проверка при
    запуске, кнопка проверки, доступная версия/заметки, прогресс, ошибки,
    «Позже».
  - Helper автоматически не обновляется: сервис лежит вне бандла; при смене
    протокола UI покажет `HELPER_PROTOCOL` и предложит переустановку.
- **Статистика трафика (S4-T4).** Локальный Clash API на `127.0.0.1`, случайный
  порт/secret на сессию, `GET /connections` с Bearer; ошибка — «—» без `ErrorBox`.
- **Подписка.** Happ-JSON, base64 и открытый текст `vless://`; UA по умолчанию
  `SplitRoute/0.1.0`, таймаут 45 с, `connect_timeout` 10 с.
- **Каталог и иконки (S4-T1/T2).** macOS 110 приложений / 100 иконок; Linux
  `.desktop`; Windows ярлыки/реестр. Иконка — отдельная `app_icon(app)`.
- **Локализация (S4-T3).** `ru`/`en` в Rust, переключение без перезапуска.
- **Helper'ы и установщики.** macOS LaunchDaemon, Linux systemd/polkit, Windows
  служба + named pipe ACL. Установка LaunchDaemon blocked подписью.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1`.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **341 тест** ok (1 `#[ignore]`).
- `cargo check`/`cargo clippy -p splitroute-helper --target
  x86_64-pc-windows-msvc --all-targets` — чисто (WFP-код компилируется, но в
  рантайме не запускался).
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **86 тестов** ok;
  `pnpm build` ok.
- **Подписанная сборка (S4-T8):** `./scripts/package.sh` с
  `TAURI_SIGNING_PRIVATE_KEY`/`..._PASSWORD` собрал `SplitRoute.app.tar.gz`
  (34.5 МиБ) и `.sig`; trusted comment подписи содержит `version:0.1.0`,
  поэтому включён `requireSignedVersion`. Обычный `pnpm tauri build` без ключа
  работает (updater-артефакты не создаются).
- `pnpm tauri dev` с `SPLITROUTE_MOCK_HELPER=1`: окно `SplitRoute` открывается,
  процесс `target/debug/splitroute` работает; после проверки остановлен, порт
  1420 свободен, `sing-box`/`splitroute` не остались.
- **S4-T6 (тесты):** TCP-probe на loopback (открытый/закрытый порт, плохое
  имя), стабильность кодов `PING_*`, минимум и порядок при равенстве, лимит
  64 профиля, гистерезис и подтверждения, серия неудач, отсутствие рабочей
  альтернативы, сброс трекера, `stop → start` при переключении, выбор без
  туннеля, восстановление прежней локации при ошибке старта, сохранение флага
  kill switch, отсутствие адресов серверов в логах, миграция старых настроек и
  нормализация диапазонов, UI-кнопка/сортировка/«Лучшая»/ошибка,
  автопереключение из UI.
- **sing-box check** и golden-тесты конфигов зелёные: конфиг ядра не менялся.
- **Kill switch (тесты):** тексты правил pf/nft, 6 тестов статического плана
  WFP (порядок весов, loopback/TUN/ядро, локальные сети, DHCP, отсутствие данных
  от клиента, фиксированные TUN-адреса), идемпотентность enable/disable,
  ошибка enable → `KILL_SWITCH_FAILED` без записи конфига, сохранение правил
  после «падения», `stop` снимает правила, serde протокола 2, mock helper,
  команда `connect` передаёт флаг, UI переключатель/состояния/ошибка.
- **Автообновление (тесты):** placeholder → `UPDATE_NOT_CONFIGURED`, URL
  канала/HTTPS, отказ от HTTP, классификация ошибок подписи/сети/конфигурации,
  стабильные коды, отсутствие `download_url`/подписи в ответе, нормализация
  канала и legacy-настройки, сохранение `preferences.updates`; UI — 7 сценариев
  `UpdateCard` (не настроено, актуально, доступная версия, установка, прогресс
  50%, ошибка подписи, «Позже») и сохранение канала/проверки в настройках.

### Не проверено (причина)

- **Реальная задержка серверов провайдера** — в сессии нет реального профиля и
  сети провайдера; тесты используют fake-probe и закрытые порты.
- **Замер на Linux/Windows** — код платформонезависим (`std::net`), но рантайм
  не запускался.
- **DNS-резолв не имеет отдельного таймаута** — `to_socket_addrs` использует
  системный resolver; при зависшем resolver проверка может быть дольше 3 с.
- **Окно между `stop` и `start` при переключении** — правила kill switch на это
  время снимаются; при ошибке старта приложение возвращает прежнюю локацию, но
  абсолютной атомарности без нового протокола helper'а нет.
- **Реальное применение pf/nft под root** — `sudo -n true` не проходит; нужна
  ручная проверка `S4-T5c`.
- **Windows kill switch в рантайме** — код и план фильтров готовы (`S4-T5b`,
  `8c67018`), но WFP не устанавливался: нет VM. Не проверены матчинг локального
  адреса TUN, блокировка прямого `curl.exe` при мёртвом ядре и снятие правил —
  `S4-T5c`. Также не проверено поведение при перезапуске BFE: статические
  объекты WFP при этом теряются, helper их не переустанавливает до следующего
  `start`.
- **Реальная установка обновления** — нет публичного HTTPS-манифеста: домен не
  выбран (PRD §13.3), а локальный self-signed TLS updater не принимает.
  Проверены подпись артефакта (`SplitRoute.app.tar.gz.sig`, версия 0.1.0) и
  UI-сценарии, но не замена `.app`, перезапуск и поведение Gatekeeper/NSIS.
- **Windows/Linux updater-артефакты** — нет VM/машины; формат создаёт Tauri
  bundler, но `.exe`/`.AppImage` с обновлением не собирались.
- **Обновление helper'а вместе с приложением** — не реализовано осознанно:
  сервис установлен вне бандла, его обновляют переустановкой из настроек.
- **Откат неудачной установки** — не реализован: updater заменяет приложение,
  отдельного rollback нет.
- **nftables на Linux** — нет машины с root/GUI; правило и классификация ошибок
  покрыты тестами, рантайм — нет.
- **Сон/пробуждение и смена сети с активным kill switch** — правила не содержат
  адресов, но рантайм не проверялся.
- **Статистика после перезапуска окна** — «—» до переподключения (осознанно).
- **Установка/запуск LaunchDaemon** — blocked: нужен Apple Development/Developer ID.
- **TUN, per-app, DNS, сон/пробуждение на упакованных бинарях** — blocked: нет
  root и реальной подписки в сессии.
- **Linux-рантайм** (S2-T9, S3-T3, S3-T7) — нет GUI/пароля sudo.
- **Windows-рантайм** (S3-T4, S4-T2, S4-T5b) — нет VM; полный
  `cargo check -p splitroute --target x86_64-pc-windows-msvc` падает на `ring`.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`, Linux —
  `gio-sys`/`glib-sys`. Платформенные ветки проверяйте изолированными крейтами.
- `sudo -n true` **не проходит**; привилегированные действия — только с явным
  подтверждением пользователя.
- **Debug-сборка** ищет сокет в `$TMPDIR/splitroute-helper.sock`. Установленный
  helper: macOS — `/var/run/splitroute/helper.sock`, Linux —
  `/run/splitroute/helper.sock`, Windows — `\\.\pipe\splitroute-helper`;
  переопределение — `SPLITROUTE_HELPER_SOCKET`/`SPLITROUTE_HELPER_PIPE`.
- **Helper: одно сообщение на соединение.** Smoke-клиент подключается заново.
- **Установленный helper v1 несовместим с протоколом 2** — после обновления
  бинарей переустановите helper; иначе `HELPER_PROTOCOL`.
- **Bundle id** `com.splitroute.app` заканчивается на `.app` — Tauri
  предупреждает; финальный идентификатор решить до публичной сборки (PRD §13.3).
- **Провайдер подписки:** формат зависит от UA; `SplitRoute/0.1.0` → base64
  `vless://` (~1 с), `Happ/4.1.0` → JSON (иногда ~17 с). Таймаут 45 с не уменьшать.
- **CachyOS доступна по SSH** (хост/каталог — в чате, не коммитить). Нет
  git-репозитория: синхронизация `rsync`. `sudo -n` требует пароль, GUI нет.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`). Секреты не логировать.
- **Автовыбор в state.json:** `preferences.auto_select`; старые файлы читаются
  с выключенными функциями (100 мс, 60 с, 2 подтверждения, 2 неудачи).
- **Clash API (S4-T4):** только `127.0.0.1`, случайный порт/secret на сессию;
  endpoint в `state.json` не сохраняется; в логи/диагностику не попадает.
- **Kill switch runtime-файлы:** `<runtime>/killswitch.pf` (macOS),
  `killswitch.nft` (Linux), `killswitch.token` (macOS) — права `0600`, удаляются
  при снятии правил. Windows файлов не пишет: правила живут в BFE (провайдер
  `SplitRoute kill switch`, собственный подслой WFP).
- **Windows/WFP-проверка:** `netsh wfp show filters file=...`; `netsh wfp show
  state` для общего состояния. Фильтры статические, не persistent.
- **Updater-ключ (S4-T8):** приватный ключ `~/.commandcode/splitroute-updater.key`
  и пароль `.key.pass` (права `0600`); публичный — в
  `src-tauri/tauri.conf.json`. В git не коммитить. `package.sh` требует
  `TAURI_SIGNING_PRIVATE_KEY` (путь или содержимое) и
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; `SPLITROUTE_ALLOW_UNSIGNED=1` собирает
  без updater-артефактов.
- **Обновления:** endpoint `https://updates.splitroute.invalid/<channel>/latest.json`
  — заглушка, проверка даёт `UPDATE_NOT_CONFIGURED`. Debug override —
  `SPLITROUTE_UPDATE_ENDPOINT` (только HTTPS). Подписанные артефакты —
  `target/release/bundle/macos/SplitRoute.app.tar.gz` и `.sig`.
- **UI-автоматизация:** System Events отвечает; окно `SplitRoute` находится.
- **Скриншоты** с реальным URL/HWID/адресом сервера в репозиторий не коммитить.
- После проверок: `pgrep -x sing-box`, `pgrep -fl 'target/debug/splitroute'`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`,
  `launchctl print system/com.splitroute.helper`.

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

## Ручные проверки, которые остаются blocked

Выполнять только при появлении root/сертификата/целевых ОС и с явным
подтверждением пользователя. Привилегированные действия автоматически не
запускать.

### macOS: kill switch (S4-T5c)

1. `pnpm tauri dev` с реальным helper'ом (не MockHelper), dev-helper под `sudo`
   — см. AGENTS §4.5; включить kill switch в настройках.
2. Подключиться, выбрать приложение с правилом, запустить в нём трафик.
3. `sudo pkill -x sing-box`. Ожидается: прямой трафик выбранного приложения
   блокируется, UI показывает «Ошибка» и кнопку «Отключить и снять защиту».
4. Нажать кнопку (или «Отключить»). Ожидается: `pfctl -a
   com.apple/splitroute.killswitch -s rules` пуст, правила сняты, прямой трафик
   восстановлен, `pfctl -s references` не содержит ссылки helper'а.
5. Проверить, что после перезапуска helper'а «залипших» правил нет.

### macOS: задержки и автопереключение (S4-T6)

1. С реальным профилем нажать «Проверить задержки»: числа должны быть
   сопоставимы с `ping`/`nc -z` до тех же серверов; ошибки — локализованные.
2. Включить автовыбор, обновить подписку — выбранная локация должна стать
   самой быстрой по TCP.
3. Включить автопереключение, подключиться, дождаться двух проверок: при
   деградации выше порога должен произойти `stop → start` без зависшего TUN.
4. При ошибке старта новой локации проверить, что вернулась прежняя и правила
   kill switch снова применены.

### Обновления (S4-T8)

Выполнять после выбора домена и хостинга манифеста (PRD §13.3).

1. Заменить placeholder в `UPDATE_BASE` (`src-tauri/src/updates.rs`) на реальный
   домен; собрать версию N и установить её. Подписать артефакт тем же ключом,
   что в `tauri.conf.json`.
2. Опубликовать `<channel>/latest.json` с версией N+1, `notes`, `pub_date`,
   `platforms` и `url`+`signature` для текущей ОС.
3. В настройках нажать «Проверить обновления»: версия, канал и заметки; кнопка
   «Установить и перезапустить» появляется, но ничего не устанавливается
   автоматически.
4. Нажать установку: ядро останавливается, прогресс растёт, приложение
   перезапускается в версии N+1.
5. Испортить подпись (заменить `.sig`): ожидается `UPDATE_SIGNATURE`, установка
   отменена, приложение не перезапускается.
6. Проверить сборку без домена: кнопка проверки недоступна, показано «канал не
   настроен», сетевых запросов нет.
7. Если протокол helper'а менялся, после перезапуска UI покажет
   `HELPER_PROTOCOL`; переустановить helper из настроек.

### Linux (S3-T3, S3-T7, S2-T9, S4-T5c)

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
pnpm install
./scripts/fetch-core.sh
./scripts/package.sh             # deb, rpm, appimage
```

Проверить установку/`systemctl`/`pkexec`/TUN/per-app/DNS/kill switch/задержки по
матрице `docs/stage-3-report.md` §9. AppImage без FUSE:
`./SplitRoute_*.AppImage --appimage-extract-and-run`.

### Windows (S3-T4, S4-T2, S4-T5b, S4-T5c)

`./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh`, затем
`./scripts/portable.sh` или `./scripts/package.sh`; проверить службу под SYSTEM,
UAC, pipe ACL, `wintun.dll`, TUN/per-app, NSIS.

Kill switch (WFP) — только на VM/машине, где не страшно потерять сеть:

1. Установить helper (служба под SYSTEM) и включить kill switch в настройках.
2. Подключиться; проверить, что браузер/`curl.exe` ходят через прокси (внешний IP
   — прокси), локальная сеть и DHCP работают.
3. Убедиться, что фильтры на месте: `netsh wfp show filters
   file=%TEMP%\splitroute-filters.xml` — в файле есть провайдер «SplitRoute kill
   switch»; блокирующие фильтры на `ALE_AUTH_CONNECT_V4/V6` имеют вес ниже
   разрешений.
4. `Stop-Process -Name sing-box -Force`. Ожидается: прямой `curl.exe
   https://ifconfig.me` не проходит, UI показывает ошибку и кнопку снятия
   защиты, правила WFP остались.
5. Нажать «Отключить и снять защиту» (или «Отключить»). Ожидается: фильтров
   SplitRoute в `netsh wfp show filters` нет, прямой трафик восстановлен.
6. Перезапустить helper: «залипших» фильтров SplitRoute быть не должно.
7. Отдельно проверить перезапуск BFE (`Restart-Service BFE`): статические
   фильтры при этом теряются; после переподключения они должны появиться снова.
   Это известное ограничение, не утечка при работающем helper'е.

---

## Промпт следующей подцели: G4.9 (S4-T11, запущенные процессы)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.9 этапа 4 — S4-T11 «Список запущенных процессов на вкладке
„Запущенные“».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3, §6–§7, §10, §12–§14, §17–§18.
- PRD.md §6.3 (allowlist и вкладки), §7 (экран «Приложения»), §8, §10 (R2, R3).
- TASKS.md: S4-T11, S4-T1, S4-T2; docs/research.md §10 (каталог) и §20–§22
  (соседние закрытые задачи — не ломай их).
- NEXT_SESSION.md, crates/protocol/src/lib.rs, src-tauri/src/catalog/,
  src-tauri/src/commands/mod.rs, src/features/apps/AppsScreen.tsx,
  src/shared/api/{client,types}.ts, src/shared/state/appStore.ts,
  src/shared/i18n/ru.ts и en.ts.

ТЕКУЩЕЕ СОСТОЯНИЕ
- S4-T1…S4-T8, S4-T9, S4-T10 закрыты. Последний коммит кода — `5172dd5`
  (автообновление).
- Вкладка «Запущенные» — заглушка с пояснением (`apps.tabs.runningHint`).
  Каталог установленных приложений и иконки работают: `InstalledApp {name, path,
  platform}`, команда `apps_installed`, `app_icon(app)`, правило из каталога
  создаётся выключенным (`app_rule_from_installed`).
- Проверено 2026-11-14: 341 Rust-тест, 86 frontend, clippy/typecheck/lint/build
  зелёные, `pnpm tauri dev` (MockHelper) открывает окно.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- Новых привилегий и shell-команд: процессы читаются из ОС непривилегированным
  Rust-кодом (macOS — `libc`/`proc_pidpath`, Linux — `/proc`, Windows — Toolhelp
  из уже подключённого `windows-sys`). Никаких `ps`/`tasklist` через shell.
- helper и его протокол не меняются; `crates/protocol` — единственный источник
  общих моделей. Новый тип процесса добавляй в `protocol`, если он нужен UI.
- Секреты, реальные URL и пути из системных каталогов в логи не пишутся.
- Ошибка чтения процессов не ломает вкладку: пустой список + локализованная
  причина, остальные экраны работают.
- Лимит списка (например, 500), дедупликация по пути, сортировка по имени;
  системные/пустые/недоступные записи пропускаются с причиной в Debug-логе.

ЗАДАЧИ
1. Модель и команда.
   - Добавь `RunningApp {pid, name, path, platform}` в `crates/protocol` (serde,
     round-trip) или обоснуй переиспользование `InstalledApp`.
   - Команда `apps_running` (`#[tauri::command(async)]`): собрать процессы
     текущей ОС, отфильтровать, дедуплицировать, отсортировать. Платформенный
     код — в `src-tauri/src/catalog/` (отдельные модули, без размазывания по
     бизнес-логике).
   - Правило из запущенного процесса создаётся выключенным: macOS —
     `path_regex` по `.app`, если путь внутри бандла, иначе `process_path`;
     Windows/Linux — `process_path`. Команда `app_rule_from_running` или общий
     помощник с тестами.
2. UI.
   - Вкладка «Запущенные»: загрузка, пустое состояние, ошибка, поиск по имени,
     «Добавить правило»; после добавления — подсказка, что правило выключено и
     включается на «Все правила».
   - Не грузи иконки для запущенных, если это заметно замедляет список; при
     желании используй существующий `app_icon` точечно.
   - Строки в `ru`/`en`; тест запрещает кириллицу в `en`.
3. Тесты.
   - Чистые функции: фильтрация системных процессов, дедупликация, лимит,
     сортировка, построение правила; фикстуры/временные каталоги там, где можно
     проверить на macOS.
   - UI: загрузка, пусто, ошибка, поиск, добавление правила; существующие
     тесты каталога не ломать.
   - Реальный скан macOS — вручную/тестом с мягкими проверками (не фиксируй
     конкретные имена процессов).
4. Проверки.
   - cargo fmt --all --check
   - cargo clippy --workspace --all-targets -- -D warnings
   - cargo test --workspace
   - pnpm typecheck && pnpm lint && pnpm test && pnpm build
   - `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc`
   - `pnpm tauri dev` (MockHelper) не должен ломаться.
5. Документация.
   - TASKS.md: S4-T11 → done со ссылкой на коммит; обнови этап 4.
   - docs/research.md: что проверено, ограничения macOS/Linux/Windows.
   - NEXT_SESSION.md: состояние и промпт следующей незакрытой задачи
     (S4-T7 после решения PRD §13.2 или backlog).

ОГРАНИЧЕНИЯ
- Не менять helper-протокол, kill switch, обновления, установщики, иконки и
  статистику сверх задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не оставлять dev-серверы и процессы после проверок.

ОТЧЁТ
- TASKS.md: статус S4-T11 со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, самодостаточный промпт следующей
  подцели.
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S4-T6 (задержки/автовыбор):** замер — TCP-connect, не прокси-запрос; в UI
  это подписано. DNS-резолв отдельным таймаутом не ограничен. Переключение —
  `stop → start`, поэтому kill switch на мгновение снимается; при ошибке старта
  возвращается прежняя локация. Не добавляйте в замер ядро или helper.
- **S4-T8 (обновления):** домен не выбран — endpoint `.invalid` даёт
  `UPDATE_NOT_CONFIGURED`; не подменяйте его публичным URL без решения PRD
  §13.3. Приватный ключ `~/.commandcode/splitroute-updater.key` не коммитить и
  не печатать; публичный ключ в `tauri.conf.json` меняется только новой
  сборкой. `requireSignedVersion` включён: подпись без версии будет отклонена.
  Helper автоматически не обновляется — переустановка из настроек.
- **S4-T5b (Windows):** правила — WFP-провайдер и подслой, не `netsh`: в
  брандмауэре Windows блок приоритетнее разрешений, allow-list так не
  выражается. Трафик приложений в TUN разрешён по локальному адресу
  (`WINDOWS_TUN_V4/V6`), потому что правила ставятся до создания wintun;
  адреса задублированы в `src-tauri/src/config/mod.rs` — меняйте вместе.
  При остановке BFE статические фильтры теряются до следующего `start`.
  В рантайме не проверено (`S4-T5c`), не выдавайте это за проверенное.
- **Kill switch (S4-T5a/T5b):** протокол 2; после обновления переустановите
  helper. Правила шире allowlist — при мёртвом туннеле прямой выход теряют все
  непривилегированные процессы; локальная сеть остаётся. Снятие — `stop`,
  выход приложения или выключение настройки. Не добавляйте в правила адреса,
  порты или данные от клиента.
- **S4-T4 (статистика):** источник — локальный Clash API; порт/secret в памяти,
  после перезапуска окна — «—» до переподключения.
- **S4-T3 (локализация):** новые строки — в `ru.ts` и `en.ts`; тест запрещает
  кириллицу в `en`. Язык меняется через `settings_update`.
- **S4-T2 (иконки):** не переносите чтение иконок в `apps_installed`.
- **S4-T1 (каталог):** macOS-скан не рекурсивный; «Запущенные» — `S4-T11`.
- **S3-T2 blocked подписью:** нужен Apple Development/Developer ID.
- **Лицензии:** при смене sing-box/wintun/updater править
  `THIRD_PARTY_NOTICES`; тест `packaging_config` это проверяет.
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`. `package.sh`
  требует `TAURI_SIGNING_PRIVATE_KEY` и `..._PASSWORD` для updater-артефактов;
  `SPLITROUTE_ALLOW_UNSIGNED=1` — сборка без них.
- **Секреты** (URL подписки, HWID, UUID, реальные адреса, `secret` Clash API,
  содержимое pf/nft-правил с адресами) в задачи, фикстуры, логи и коммиты не
  попадают.
