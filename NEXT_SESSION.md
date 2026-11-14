# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–2 закрыты (в этапе 2 S2-T9 blocked: нет CachyOS).
- **Этап 3: S3-T1, S3-T3, S3-T4, S3-T5, S3-T6, S3-T8 — done.**
  **S3-T2 — blocked** (нет сертификата Apple). **S3-T7 — blocked частично:**
  macOS-часть выполнена; на CachyOS собраны `.deb`/`.rpm`/AppImage и прогнаны
  тесты (см. ниже), но установка, `systemctl`/`pkexec`, TUN и GUI не проверены;
  Windows VM нет.
- **Этап 4: S4-T1, S4-T2, S4-T9, S4-T10 — done.** Следующая — S4-T3
  (EN-локализация).
- Дата проверки: **2026-11-14**. Ветка `main`. Последние коммиты: `9fd1bdf`
  (feat: base64/vless), `06701bf` (fix: зависание подписки), `1b1b9fe` (docs
  G4.2), `ae1fe6d` (feat: иконки). Отчёт этапа 3:
  [docs/stage-3-report.md](./docs/stage-3-report.md) §9. Факты:
  [docs/research.md](./docs/research.md) §13–§17.
- Системные сервисы, сокеты, пайпы, dev-серверы и процессы после проверок не
  остались.

### Что работает

- **Helper, оба транспорта.** `crates/helper`: Unix-сокет `0600` (uid пира
  `getpeereid`/`SO_PEERCRED`) и Windows named pipe
  `\\.\pipe\splitroute-helper` с SDDL-ACL на SID пользователя, SYSTEM и
  Administrators + сверка SID клиента. Протокол: `status`/`start`/`stop`;
  `install`/`uninstall` по IPC запрещены. **Одно сообщение на соединение** —
  клиент переподключается перед каждой командой.
- **Установщики.** macOS (`/Library/PrivilegedHelperTools`, LaunchDaemon,
  `osascript`), Linux (`/usr/lib/splitroute`, systemd-юнит `0711`/`UMask=0077`,
  `pkexec`), Windows (`C:\Program Files\SplitRoute`, служба под SYSTEM,
  named pipe ACL, `icacls`, UAC). macOS/Linux после установки ждут сокет.
- **UI установки.** Коды `INSTALLER_PRIVILEGES`, `INSTALLER_UNSUPPORTED`,
  `INSTALLER_MISSING_BINARY`, `INSTALLER_FAILED`; `ErrorBox` показывает
  локализованный заголовок и точный текст установщика. Состояния helper'а:
  установлен / не установлен / устарел (`HELPER_PROTOCOL`) / не отвечает
  (`HELPER_TIMEOUT`/`HELPER_UNAVAILABLE`) — `src/shared/lib/helperState.ts`.
- **Подписка (S4-T9, S4-T10).** `subscription_fetch` — `#[tauri::command(async)]`:
  блокирующий HTTP ушёл в thread pool, окно не замерзает. Таймаут 45 с,
  `connect_timeout` 10 с. Формат тела определяется автоматически: Happ-JSON,
  base64 или открытый текст `vless://` (Reality/TLS, `pbk`/`sid`/`sni`/`fp`/
  `alpn`, `flow`, TCP, IPv6; остальное пропускается с причиной). User-Agent
  по умолчанию — собственный `SplitRoute/0.1.0` (провайдер отдаёт base64
  быстрее и стабильнее); `Happ/4.1.0` остаётся опцией. Сохранённый UA не
  перезаписывается, миграции нет. Неизвестное тело → `INVALID_FORMAT`.
  base64/vless больше не backlog (PRD §2 обновлён).
- **Каталог приложений (S4-T1).** `crates/protocol::InstalledApp {name, path,
  platform}`; команда `apps_installed` (`src-tauri/src/catalog/`):
  - macOS — `.app` в `/Applications`, `/System/Applications`, `~/Applications`,
    один уровень без рекурсии; имя — stem бандла;
  - Linux — `.desktop` в `/usr/share/applications` и
    `~/.local/share/applications`; имя из `Name`, путь из первого токена `Exec`
    (снимается `env VAR=…`, отбрасываются field-коды); относительные и
    отсутствующие бинари пропускаются;
  - Windows — ярлыки меню «Пуск» (ProgramData + APPDATA, рекурсивно) и
    `Uninstall` в HKLM/HKCU/WOW6432Node по `DisplayIcon`; свой разбор Shell Link.
  - Битые записи пропускаются с причиной в лог `Debug`, результат
    дедуплицируется по `path` и сортируется по имени.
  - `app_rule_from_installed` создаёт **выключенное** правило: macOS —
    `path_regex ^<escaped .app>/`, Windows/Linux — `process_path`.
  - UI: вкладки «Запущенные / Установленные / Все правила» (PRD §7), поиск по
    имени, пустые состояния, типизированная ошибка с повтором.
- **Иконки каталога (S4-T2).** `InstalledApp` не менялся; отдельная команда
  `app_icon(app)` (`#[tauri::command(async)]`) отдаёт готовый `data:` URL или
  `null`:
  - macOS — PNG-чанк из `.icns` в `Contents/Resources` (минимальная ширина
    ≥ 64; `Info.plist` не парсится);
  - Linux — `Icon=` из `.desktop`, поиск в `hicolor` по размерам 64/48/128/
    256/32/512, затем `scalable` (SVG), затем `/usr/share/pixmaps`;
  - Windows — `PrivateExtractIconsW` 64×64 (fallback `ExtractIconExW`),
    `GetDIBits` BGRA, прозрачность из AND-маски.
  - Нормализация до 64 px (`image`, png-only), лимит 256 КиБ, свой base64;
    кэш в памяти по пути, негативный результат тоже; карта `exec → Icon` на
    Linux строится один раз.
  - UI: `AppIcon` 32×32, ленивая загрузка по `IntersectionObserver`
    (`rootMargin: 200px`), модульный кэш по `app.path` (включая «иконки нет»),
    заглушка с первой буквой; ошибка иконки не показывает `ErrorBox`.
- **Лицензии.** `THIRD_PARTY_NOTICES` в корне; `LICENSE` и notices в бандле
  (macOS — `Contents/Resources`, `.deb`/`.rpm`/AppImage —
  `/usr/share/doc/splitroute/`, NSIS — `licenseFile`). Новых пакетов S4-T2 не
  добавил: `image` уже был транзитивно через `tauri/image-png`.
- **Dev-режим.** `SPLITROUTE_MOCK_HELPER=1` сохранён на всех ОС.

### Проверено (2026-11-14)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  — чисто. `cargo test --workspace` — **255 тестов** ok (1 `#[ignore]` —
  реальный скан иконок).
- `pnpm typecheck`, `pnpm lint` — чисто; `pnpm test` — **51 тест** ok;
  `pnpm build` ok.
- Подписка (S4-T9/S4-T10) на реальном провайдере: с `SplitRoute/0.1.0` —
  `200`, `text/plain`, base64 (844 байта, время 1.13–16.83 с в разных
  запросах); реальное тело разобрано парсером → **2 профиля, 0 пропусков**,
  оба Reality/TCP. С `Happ/4.1.0` — JSON 6044 байта за 17.15 с. Сервер
  нестабилен: тот же запрос иногда висит >20–30 с; клиент больше не
  блокирует окно и ждёт до 45 с.
- Linux (CachyOS, реальная машина): `cargo test --workspace` — 168 lib +
  интеграционные, 0 failed, 1 ignored; реальный скан каталога — **19
  приложений, 18 иконок** (холодные 0.69 с, кэш 19 µs). `./scripts/package.sh`
  — `.deb`/`.rpm` **33.47 МиБ**, AppImage **136.99 МиБ**; внутри AppImage
  `splitroute`, `sing-box`, `splitroute-helper`, `LICENSE`,
  `THIRD_PARTY_NOTICES`, `.desktop`; `strings` подтверждает `INVALID_FORMAT`
  и `SplitRoute/0.1.0`.
- Реальный скан macOS: **110 приложений**, **100 иконок**. `cargo test -p
  splitroute --lib catalog::icons::tests::real_catalog_icons_are_found --
  --ignored`: скан 0.4 мс, холодная загрузка всех иконок 1.35 с (debug), из
  кэша 0.12 мс; суммарно 659 КиБ, максимум 12 КиБ на иконку.
- Реальное окно `pnpm tauri dev` (`SPLITROUTE_MOCK_HELPER=1`): вкладка
  «Установленные» показывает иконки Adguard, Antigravity, AnyDesk, App Store
  32×32 и заглушки без ошибок. Автоматизировать клики не удалось: System
  Events отвечает `-1728` (нет разрешения Accessibility) — проверка визуальная
  по скриншоту, вкладку временно открывали сменой `initialState.activeTab`.
- Windows-код иконок (`windows_icons.rs`) типизирован изолированно:
  `cargo check --all-targets --target x86_64-pc-windows-msvc` с
  `windows-sys 0.61` — ок. Полный `cargo check -p splitroute` под MSVC
  по-прежнему падает на `ring` (`assert.h not found`).
- Windows-ветка реестра (`collect`/`uninstall_roots`/`read_uninstall_key`)
  типизирована отдельной проверкой (S4-T1).

### Не проверено (причина)

- **Загрузка подписки из UI после S4-T10** — клик в окне недоступен (System
  Events `-1728`); проверены curl с обоими UA, парсер и e2e-тест через
  локальный HTTP-сервер. Миграции UA нет, перезапуск ничего не меняет.
- **Linux-рантайм** — пакеты и AppImage собраны, `cargo test` и реальный
  скан иконок прошли, но установка (`dpkg`/`systemctl`/`pkexec`), запуск GUI,
  TUN/per-app и DNS не проверены: по SSH нет GUI, `sudo -n` требует пароль.
- **Иконки Linux** — проверены на реальном скане: 19 приложений, 18 иконок,
  холодные 0.69 с, кэш 19 µs. Не проверены визуально в окне и SVG/XPM в
  живых темах (SVG отдаётся как `data:image/svg+xml`, XPM не поддержан).
- **Иконки Windows в рантайме** — нет Windows VM: проверены компиляция
  Win32-вызовов и чистые функции BGRA/маски. Поведение `GetDIBits` на реальных
  `.exe` (и допущение «белый пиксель маски = прозрачный») не подтверждено.
- **10 из 110 приложений macOS без иконки** — предел источника: у 7
  wrapper-бандлов нет своего `.icns`, у Calendar/Photo Booth/System Settings
  иконка только в `Assets.car`. В UI — заглушка, не ошибка.
- **Установка/запуск LaunchDaemon** — blocked: `security find-identity -v -p
  codesigning` → 0; нужен Apple Development/Developer ID. Чек-лист — отчёт §6.
- **TUN, per-app, DNS, сон/пробуждение, смена сети на упакованных бинарях** —
  blocked: `sudo` требует пароль, реальной подписки в сессии нет.
- **Linux/CachyOS (S3-T3, S2-T9):** `systemctl`, `pkexec`, права
  `/run/splitroute`, `.deb`/`.rpm`/AppImage — нет машины. Каталог `.desktop`
  и иконки на реальном Linux тоже не проверены.
- **Windows (S3-T4):** служба под SYSTEM, `sc.exe`, UAC, pipe ACL, TUN,
  `wintun.dll`, NSIS — нет VM. Реальные ярлыки, реестр `Uninstall` и иконки
  `.exe` не проверены, только фикстуры и компиляция.
- **Пометка несуществующих путей у сохранённых правил** (PRD §6.3) — не
  реализована; вне объёма S4-T1/S4-T2.

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
- Установлены цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`.
- **Кросс-`cargo check -p splitroute` невозможен:** Windows — `ring`
  (`assert.h not found`), Linux — `gio-sys`/`glib-sys`. Для Windows-веток
  каталога/иконок проверяйте функции изолированным крейтом с `windows-sys 0.61`
  и `--target x86_64-pc-windows-msvc`.
- `sudo -n true` **не проходит**; системные службы не ставить без явного
  действия пользователя.
- **Debug-сборка** ищет сокет в `$TMPDIR/splitroute-helper.sock`. Для
  установленного helper'а: macOS — `/var/run/splitroute/helper.sock`
  (`SPLITROUTE_HELPER_SOCKET`), Linux — `/run/splitroute/helper.sock`,
  Windows — `\\.\pipe\splitroute-helper` (`SPLITROUTE_HELPER_PIPE`).
- **`externalBin` только в платформенных конфигах**; инвариант закреплён
  `src-tauri/tests/packaging_config.rs`. Перед `tauri build` нужны
  `./scripts/fetch-core.sh` (и `fetch-wintun.sh` на Windows).
- **Helper: одно сообщение на соединение.** Smoke-клиент должен подключаться
  заново на каждую команду.
- **Bundle id `com.splitroute.app` заканчивается на `.app`:** Tauri
  предупреждает о конфликте с расширением бандла. До публичной сборки
  зафиксировать финальный идентификатор (PRD §13.3); смена затронет каталог
  состояния и имена служб.
- Установщик ищет helper и sing-box: `SPLITROUTE_HELPER_BIN` /
  `SPLITROUTE_SING_BOX_BIN`, рядом с приложением, либо в
  `src-tauri/binaries/sing-box-<triple>` (dev).
- Секреты (URL подписки, HWID, UUID) не логировать и не коммитить.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`, Linux —
  `~/.config/splitroute/state.json`). UA по умолчанию — `SplitRoute/0.1.0`;
  сохранённое значение не мигрирует и не перезаписывается (S4-T10).
- **Провайдер подписки:** формат зависит от UA — `SplitRoute/0.1.0` отдаёт
  base64-список `vless://` (обычно ~1 с), `Happ/4.1.0` — JSON (может идти
  ~17 с). Сервер нестабилен: тот же запрос иногда висит >20–30 с без ответа.
  Таймаут 45 с, `connect_timeout` 10 с; не уменьшать. Поддерживаются оба
  формата, JSON-путь не единственный.
- **Linux-машина CachyOS доступна** (пользователь выдал SSH-доступ
  2026-11-14; хост/каталог — в чате, в репозиторий не коммитить). Там нет
  git-репозитория: исходники синхронизируются `rsync`; `pnpm`, Rust и
  webkit2gtk-4.1 установлены; **`sudo -n` требует пароль**, поэтому установка
  пакетов и `systemctl`/`pkexec` не выполнялись. Сборка AppImage — только на
  этой машине (`scripts/package.sh`), с macOS не кросс-собирается.
  Артефакты: `target/release/bundle/{deb,rpm,appimage}`; AppImage 137 МиБ
  (внутри `sing-box` 78 МиБ), `.deb`/`.rpm` 33.5 МиБ.
- **`THIRD_PARTY_NOTICES` обновляется вручную** (раздел 11 файла):
  `cargo metadata --all-features`, `pnpm licenses list --prod`.
- **Каталог приложений:** пути читаются из системных списков; привилегий нет.
  Не добавляйте рекурсию в macOS-скан молча — контейнеры (`Utilities`) дадут
  неверные имена.
- **Иконки:** кэш в Rust и в UI живёт до перезапуска процесса; «Обновить
  список» его не сбрасывает (осознанно — иконка приложения меняется только с
  обновлением самой программы). `.icns` без PNG-чанков, wrapper-бандлы и
  `Assets.car` дают `null` → заглушка, это ожидаемо.
- **UI-тексты** сейчас только `ru` в `src/shared/i18n/ru.ts`; `errorText`
  берёт локализованный заголовок по коду, а `message` от Rust показывается
  как есть (русский).
- После проверок: `pgrep -fl 'splitroute-helper|sing-box run'`,
  `pgrep -fl 'target/debug/splitroute'`, `ls -d /tmp/sr-*`,
  `launchctl print system/com.splitroute.helper`, `lsof -nP -iTCP:1420`.

### Проверка перед стартом

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
git status --short && git log --oneline -3
pnpm install
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test
pgrep -fl 'splitroute-helper|sing-box run' || echo "процессов нет"
```

---

## Ручные проверки, которые остаются blocked

Выполнять только при появлении сертификата/целевых ОС и с явным подтверждением
пользователя. Привилегированные действия автоматически не запускать.

### macOS: LaunchDaemon (S3-T2)

Нужен сертификат Apple Development/Developer ID; ad-hoc не подходит (AMFI:
`has no CMS blob`). Подписать helper, затем пройти чек-лист
`docs/stage-3-report.md` §6: `codesign -dv` (не `adhoc`), установка через UI,
`launchctl print system/com.splitroute.helper`, права `/var/run/splitroute`
(`drwx--x--x root wheel`) и сокета (`srw------- <пользователь>:wheel`),
подключение/отключение, удаление, отсутствие процессов.

### Linux (S3-T3, S2-T9, S4-T2)

Сборка пакетов (CachyOS/Arch: нужны `patchelf`, `fuse2`, `webkit2gtk-4.1`,
`libappindicator-gtk3`, `librsvg`; Node 22 + pnpm + rustup — AGENTS §5):

```bash
cd app
export PATH="$HOME/.cargo/bin:$PATH"
pnpm install
./scripts/fetch-core.sh          # sing-box под linux-x64
./scripts/package.sh             # deb, rpm, appimage
# артефакты: target/release/bundle/{deb,rpm,appimage}/
```

На macOS AppImage не собирается: `scripts/package.sh` выбирает цели по ОС,
кросс-сборка Tauri-бандлов не поддерживается. Запуск AppImage без FUSE:
`./SplitRoute_*.AppImage --appimage-extract-and-run`.

**Сделано 2026-11-14 на CachyOS:** сборка прошла (`.deb`/`.rpm` 33.47 МиБ,
AppImage 136.99 МиБ), `cargo test --workspace` зелёный (168 lib +
интеграционные), реальный скан иконок — 19 приложений / 18 иконок. Внутри
AppImage: `usr/bin/{splitroute,sing-box,splitroute-helper}`,
`usr/lib/SplitRoute/{LICENSE,THIRD_PARTY_NOTICES}`,
`usr/share/applications/SplitRoute.desktop`. **Не сделано:** установка,
`systemctl`/`pkexec`, запуск GUI, TUN/per-app — `sudo -n` требует пароль,
по SSH нет дисплея.

Оставшиеся проверки (нужен пароль администратора и/или GUI на машине):

```bash
# вариант без установки: AppImage (FUSE2 установлен)
./target/release/bundle/appimage/SplitRoute_*.AppImage
# или с распаковкой, если FUSE недоступен
./target/release/bundle/appimage/SplitRoute_*.AppImage --appimage-extract-and-run

# после установки helper'а из UI (pkexec):
systemctl status splitroute-helper.service
ls -ld /run/splitroute
stat -c '%a %U:%G %n' /run/splitroute/helper.sock
pgrep -a -f 'splitroute-helper|sing-box run'
```

Ожидается: `LICENSE`/`THIRD_PARTY_NOTICES` в `/usr/lib/SplitRoute` (AppImage)
или `/usr/share/doc/splitroute` (deb/rpm); сервис `active (running)`; каталог
`711 root:root`; сокет `600` пользователя; `--allow-uid` = uid установившего.
Для S4-T1/S4-T2: вкладка «Установленные» показывает приложения из
`/usr/share/applications` и `~/.local/share/applications`, иконки из
`hicolor` (PNG и SVG) отрисованы, поиск фильтрует, битые `.desktop` попадают
в логи (`Debug`) и не ломают список; приложение без `Icon=` показывает
заглушку. Отдельно проверить Flatpak/Snap-приложения: путь из `Exec` может
быть sandbox-путём.

> На CachyOS/Arch `.deb` и `.rpm` не ставятся напрямую — используйте AppImage
> или соберите пакет для Arch отдельно. `.deb`/`.rpm` проверяются на
> Ubuntu/Fedora.

### Windows (S3-T4, S4-T2)

На Windows 10/11 x64 с VS Build Tools, Rust MSVC, Node 22 + pnpm, WebView2 и
Git Bash:

```bash
cd app
rustup target add x86_64-pc-windows-msvc
./scripts/fetch-core.sh && ./scripts/fetch-wintun.sh
./scripts/portable.sh          # распаковать и запустить start.bat (UAC)
./scripts/package.sh           # NSIS-установщик
```

Проверить: `sc.exe query splitroute-helper-service` → `RUNNING` под SYSTEM;
`wintun.dll` рядом с `sing-box.exe`; pipe доступен только SID пользователя;
NSIS показывает страницу лицензии; `LICENSE` и `THIRD_PARTY_NOTICES` в
каталоге приложения; TUN поднимается, выбранное приложение идёт через прокси.
Для S4-T1/S4-T2: каталог находит ярлыки «Пуск» и записи `Uninstall`, правило
создаётся с `process_path` к `.exe`; иконки `.exe` отрисованы с прозрачным
фоном (проверить старые `.exe` с 24bpp-ресурсом — там альфа берётся из маски).

---

## Промпт следующей подцели: G4.3 (S4-T3, EN-локализация)

Скопируйте текст ниже целиком в новую сессию. Запускайте из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: подцель G4.3 этапа 4 — S4-T3 «EN-локализация».
После подцели остановись: коммит кода/тестов, отдельный коммит документации,
обновление NEXT_SESSION.md. Следующие задачи этапа 4 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3, §10 (п. 8 — единый источник состояния в Rust), §12–§14, §17–§18.
- PRD.md §6.6 (настройки), §7 (экраны), §8 (локализация).
- TASKS.md: S4-T3 и этап 4.
- NEXT_SESSION.md (этот файл), docs/research.md §15.
- src/shared/i18n/ru.ts, src/app/, src/features/*, src/shared/ui/,
  src/shared/api/{client,types}.ts, src/shared/lib/helperState.ts,
  src-tauri/src/store.rs, src-tauri/src/commands/mod.rs (CommandError),
  src-tauri/src/privileged/installer*.rs.

ТЕКУЩЕЕ СОСТОЯНИЕ
- Все видимые строки UI уже вынесены в `src/shared/i18n/ru.ts`; `t(key, vars)`
  подставляет `{vars}`, `errorText(code)` даёт локализованный заголовок по
  стабильному коду. Словарь — один (`ru`), `MessageKey = keyof typeof ru`.
- Ошибки Rust приходят как `CommandError { code, message }`; `ErrorBox`
  показывает локализованный заголовок по коду и `message` установщика как
  есть (сейчас по-русски). Коды ошибок стабильны и не переводятся.
- Проверено 2026-11-14: 255 Rust + 51 frontend тестов, clippy -D warnings,
  typecheck/lint/build зелёные. Иконки каталога (S4-T2) и подписка
  (S4-T9/S4-T10) готовы, их не трогай.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- `crates/protocol` — единственный источник общих типов; формат IPC helper'а
  (status/start/stop) не меняется.
- Новые поля моделей — только после правки PRD/типа в том же изменении.
- UI не получает root, не вызывает shell и не сканирует систему в обход
  Tauri-команд.
- Секреты не логировать и не коммитить. Новых тяжёлых зависимостей не
  добавлять (AGENTS.md §3): i18n-фреймворк не нужен, словари — свои объекты.
- Комментарии — на русском, идентификаторы — английские. Все видимые строки —
  только через словари, без захардкоженного текста в компонентах.

ЗАДАЧИ
1. Словари и типы.
   - `ru` остаётся эталоном ключей; добавить `en: Record<MessageKey, string>`.
   - `t(key, vars)` выбирает словарь по текущему языку; `errorText(code)` —
     тоже. Ключ, отсутствующий в `en`, не должен молча падать: тест
     равенства наборов ключей обязателен.
   - Язык: `ru` и `en`; значения — `"ru" | "en"`, serde-friendly.
2. Хранение языка (решение зафиксировать в PRD до кода).
   - Предпочтительно — единый источник состояния в Rust (AGENTS §10.8):
     `AppSettings.preferences.language` с `#[serde(default)]` = `ru`, чтобы
     старые `state.json` читались. UI переключает язык в настройках, применяет
     без перезапуска, значение сохраняется через `settings_update`.
   - Проверить, что смена языка не сбрасывает остальные настройки и не
     перезагружает окно; миграция старого state без поля — тест serde.
   - Если выберешь localStorage вместо Rust — обосновать в PRD/отчёте, но
     правило «единое состояние в Rust» тогда нарушается; по умолчанию — Rust.
3. UI.
   - Переключатель языка в «Настройках» (русский/английский), подпись и
     подсказка — через словари.
   - Все экраны (Главная, Локации, Приложения, Настройки, Логи), пустые
     состояния, ошибки, тултипы/`aria-label`, кнопки трея — без русских
     литералов в компонентах. Трей (`src-tauri/src/tray.rs`) — отдельно:
     решить, нужен ли перевод пунктов меню; если да, Rust должен отдавать
     строки по выбранному языку, если нет — зафиксировать в PRD/отчёте.
   - `message` от Rust (установщик, helper) остаётся как есть, если не
     добавляешь маппинг кодов; тогда в EN-режиме он показывается как деталь,
     а заголовок — локализованный. Это нужно явно описать в отчёте.
4. Тесты.
   - Frontend: наборы ключей `ru`/`en` совпадают; переключение языка меняет
     видимый текст (например заголовок экрана и подпись кнопки); отсутствие
     непереведённых ключей; `errorText` для EN; сохранение выбора через
     `settingsUpdate`.
   - Rust: serde `language` (старый state без поля → `ru`, round-trip,
     неизвестное значение → ошибка или default — решить и покрыть тестом).
5. Документация.
   - PRD §6.6/§8: язык интерфейса, где хранится, что EN покрывает (UI;
     сообщения ядра — по решению).
   - docs/research.md: новый раздел с проверенными фактами.
   - TASKS.md: S4-T3 → done со ссылкой на коммит; NEXT_SESSION.md — состояние
     и промпт G4.4 (S4-T4, статистика трафика).

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- Реальный запуск `pnpm tauri dev` (MockHelper): переключить язык, пройти все
  пять экранов, убедиться, что русского текста не осталось (кроме
  backend-деталей по решению), перезапустить приложение — выбор сохранился.
  Автоматизация UI недоступна (System Events `-1728`), поэтому проверка
  визуальная по скриншотам; вкладку можно временно открыть сменой
  `initialState.activeTab`, затем обязательно вернуть и проверить `git diff`.

ОГРАНИЧЕНИЯ
- Не менять протокол helper'а, установщики, иконки и экраны сверх локализации.
- Не выполнять привилегированные действия.
- Не выдавать непроверенное на других ОС за проверенное.

ОТЧЁТ
- TASKS.md: S4-T3 → done со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, и самодостаточный промпт G4.4
  (S4-T4, статистика трафика сессии).
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **S3-T2 blocked по подписи:** нужен Apple Development/Developer ID. Установщик
  проверяет подпись до копирования; helper в бандле ad-hoc/linker-signed.
- **S3-T7 blocked:** Linux/Windows-пункты не проверены. macOS-часть — отчёт §9.
- **TUN на упакованных бинарях** не проверен: нет root и подписки. Не регресс —
  dev-бинари проверены в S2-T8/S2-T10.
- **Bundle id** `com.splitroute.app` даёт предупреждение Tauri (суффикс `.app`).
  Решить до публичной сборки (PRD §13.3).
- **Лицензии:** `THIRD_PARTY_NOTICES` обновляется вручную; при смене версии
  sing-box/wintun править разделы 2–3 и ссылки, иначе `packaging_config`-тест
  упадёт — это намеренно. `image` уже был в списке транзитивных зависимостей,
  поэтому S4-T2 notices не менял.
- **Упаковка:** `externalBin` только в платформенных конфигах
  (`packaging_config.rs`); перед `tauri build` — `fetch-core.sh` и
  `build-helper-sidecar.sh`.
- **Smoke helper'а:** одно сообщение на соединение; клиент приложения
  переподключается на каждую команду.
- **Каталог приложений:** macOS-скан намеренно не рекурсивный (контейнеры
  `Utilities` дали бы неверные имена). `Exec`/`DisplayIcon` нормализуются
  чистыми функциями — их и расширяйте тестами, а не системными вызовами.
- **Иконки:** не переносить чтение иконок в `apps_installed` — ответ вырастет
  на сотни килобайт. Ленивая загрузка + кэш по пути — осознанное решение.
  Не добавляйте `Assets.car`/wrapper-бандлы молча: это отдельный парсер
  compiled asset catalog; сейчас такие приложения дают заглушку.
- **Подписка:** формат зависит от UA. `SplitRoute/0.1.0` (дефолт) → base64
  `vless://` (обычно ~1 с), `Happ/4.1.0` → Happ-JSON (может идти ~17 с).
  Поддерживаются оба; сервер нестабилен, таймаут 45 с. Не возвращайте
  миграцию UA и не уменьшайте таймаут. base64-парсер покрывает TCP и
  Reality/TLS; ws/grpc/http и security=none пропускаются — расширять при
  появлении таких подписок.
- **AppImage 137 МиБ** больше цели PRD 80 МиБ: внутри `sing-box` 78 МиБ и
  `splitroute` 22 МиБ без strip. Оптимизация размера — отдельная задача, не
  блокер функциональности. `.deb`/`.rpm` 33.5 МиБ.
- **Linux-машина:** git-репозитория нет, исходники синхронизируются `rsync`;
  не удаляйте `target` без нужды — сборка инкрементальная. `sudo` с паролем,
  поэтому установка и сервисы — только с участием пользователя.
- **Иконка приложения:** финал `splitroute-icon-pack/final/splitroute-1024.png`
  (вне репозитория) прогнан через `pnpm tauri icon`; в `src-tauri/icons` —
  `icon.icns`, `icon.ico`, 32/64/128/128@2x/512 и Windows Store, `bundle.icon`
  обновлён. Трей: macOS — `tray-template.png` + `icon_as_template(true)`,
  Windows/Linux — цветная `32x32.png` (белая моно на светлой теме не видна).
  Для замены — снова 1024×1024 PNG RGBA и `pnpm tauri icon <path>`. Windows
  `.ico` в рантайме не проверен.
- **`--allow-uid`/`--allow-sid`** фиксируют одного пользователя (G3.1).
- Подпись нужна для распространения: Developer ID + notarization (macOS),
  Authenticode/SmartScreen (Windows), подпись пакетов (Linux); `wintun.dll` —
  только официальная подписанная.
