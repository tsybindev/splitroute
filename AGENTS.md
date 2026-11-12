# AGENTS.md — разработка SplitRoute

Документ для разработчиков и AI-агентов. Здесь: стек, архитектура, поднятие среды на macOS и Linux, соглашения и Definition of Done. Продуктовые требования — в [PRD.md](./PRD.md), проверенные технические факты — в [docs/research.md](./docs/research.md).

**Главное правило:** сначала меняем PRD (если меняется поведение продукта), затем код. PRD и AGENTS.md не должны противоречить друг другу.

---

## 1. Что за проект

Кроссплатформенный клиент VLESS/Reality с выборочной маршрутизацией: выбранные приложения ходят через прокси, остальные — напрямую. UI на Tauri 2 + React, системная часть на Rust, ядро — sing-box как sidecar-процесс, запускаемый привилегированным helper'ом.

Название — `SplitRoute`, идентификаторы пакетов и сервисов — `splitroute`. Лицензия — **GPL-3.0** (файл `LICENSE` в корне репозитория).

---

## 2. Целевая структура репозитория

Создаётся по мере этапов; на этапе 0 существует только документация.

```
app/
├── PRD.md
├── AGENTS.md
├── TASKS.md                     # реестр этапов и задач для оркестратора
├── NEXT_SESSION.md              # готовый промпт для следующей сессии
├── README.md
├── LICENSE
├── agents/                      # переносимые описания субагентов (источник истины)
│   ├── rust-core.md
│   ├── react-ui.md
│   ├── platform-native.md
│   └── verifier.md
├── .commandcode/                # локальная проекция агентов, не в git
├── docs/
│   ├── research.md
│   └── spikes/                  # отчёты этапа 1
├── package.json                 # фронтенд + скрипты
├── pnpm-lock.yaml
├── vite.config.ts
├── tsconfig.json
├── index.html
├── src/                         # React + TypeScript
│   ├── main.tsx
│   ├── app/                     # App, провайдеры, навигация по вкладкам
│   ├── features/
│   │   ├── connect/             # главный экран, статус, таймер
│   │   ├── profiles/            # подписка, локации, задержки
│   │   ├── apps/                # allowlist, каталог приложений
│   │   ├── settings/
│   │   └── logs/
│   └── shared/
│       ├── api/                 # обёртки Tauri commands и событий
│       ├── ui/                  # базовые компоненты
│       ├── lib/                 # утилиты
│       └── i18n/                # словари ru (en позже)
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── binaries/                # sing-box-<target-triple>, НЕ в git
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands/            # тонкие Tauri commands
│   │   ├── core/                # состояние, запуск ядра, health
│   │   ├── subscription/        # загрузка и парсинг Happ JSON
│   │   ├── routing/             # AppRule -> правила sing-box
│   │   ├── config/              # генератор конфига
│   │   ├── privileged/          # клиент helper'а, установка
│   │   └── logging.rs
│   └── tests/                   # golden-тесты конфигов, парсера
├── crates/
│   ├── protocol/                # типы IPC app <-> helper
│   └── helper/                  # привилегированный helper
├── scripts/
│   ├── core-version.txt         # версия sing-box + sha256 по платформам
│   ├── fetch-core.sh
│   ├── build-helper-sidecar.sh  # helper под именем <triple> для externalBin
│   ├── package.sh               # пакет текущей ОС (deb/rpm/appimage, nsis, app/dmg)
│   ├── dev-helper.sh
│   ├── sync-agents.sh           # проекция agents/ в каталог харнесса
│   └── check-config.sh
├── Cargo.toml                   # workspace: src-tauri, crates/*
└── .gitignore
```

Правила раскладки:

- UI не содержит системной логики и не знает про sing-box напрямую.
- Вся привилегированная работа — только в `crates/helper`.
- Общие типы IPC — только в `crates/protocol`, без дублирования структур.
- Секреты и сгенерированные конфиги — в пользовательских каталогах, не в репозитории.

---

## 3. Стек и зависимости

**Frontend:** React 19, TypeScript, Vite, Tailwind CSS 4, Zustand (состояние), lucide-react (иконки). UI-примитивы — точечно Radix, без тяжёлых kit'ов. Роутера нет: одна окно и вкладки. Тесты — Vitest + Testing Library.

**Rust:** `tauri` 2, `tokio`, `serde`/`serde_json`, `reqwest` (rustls), `sysinfo`, `thiserror`, `anyhow`, `tracing`/`tracing-subscriber`, `uuid`, `directories`, `tokio-util`. Для helper: `interprocess` или нативные Unix socket / named pipe, `windows-service` (Windows), `libc`/`nix` (Unix). Версии фиксируются в lock-файлах и в `scripts/core-version.txt` для ядра.

**Ядро:** sing-box, версия закреплена, бинарь скачивается скриптом с проверкой SHA256. В git бинарники не коммитим.

**Лицензия:** GPL-3.0. При поставке сборок прикладываем `LICENSE`, `THIRD_PARTY_NOTICES` (sing-box, Tauri-плагины, шрифты/иконки) и ссылку на исходники используемой версии ядра. Код из GPL-3.0-проектов (Clash Verge Rev, FlClash) можно переносить только с сохранением авторства и лицензии.

**Чего не добавляем без обоснования:** Electron, ORM/БД, тяжёлые UI-kit'ы, state-менеджеры-комбайны, i18n-фреймворки, собственный packet-стек, второй VPN-протокол «на всякий случай».

---

## 4. Поднятие среды: macOS

Проверено на macOS 15.8.2 arm64 (Xcode CLT, Node 22 и pnpm уже установлены). Минимально: macOS 13+.

### 4.1. Базовые инструменты

```bash
xcode-select --install        # если ещё не установлено
xcode-select -p               # должно вывести путь к Developer dir
```

### 4.2. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
rustup target add aarch64-apple-darwin x86_64-apple-darwin
rustc --version && cargo --version
```

### 4.3. Node и pnpm

```bash
# вариант через fnm (рекомендуется)
brew install fnm
eval "$(fnm env)"
fnm install 22 && fnm use 22

# или использовать уже установленный Node 22
node --version                  # 22.x
corepack enable
corepack prepare pnpm@latest --activate
pnpm --version
```

### 4.4. Зависимости проекта и запуск

```bash
cd app
pnpm install
./scripts/fetch-core.sh          # скачает sing-box под текущую платформу
pnpm tauri dev                   # UI + ядро; helper берётся из сокета (§4.5)
```

Пока helper не установлен (этап 3), подключение можно проверить в debug-сборке
без root и TUN — `MockHelperClient`:

```bash
SPLITROUTE_MOCK_HELPER=1 pnpm tauri dev
```

Прототип helper'а из спайка запускается отдельно. Для TUN — под `sudo`; сокет
передаётся пользователю через `SUDO_UID`/`SUDO_GID`, клиент с его uid допускается:

```bash
sudo python3 spike/helper/helper.py \
  --socket /tmp/sr-helper.sock \
  --runtime-dir /tmp/sr-helper \
  --sing-box ./src-tauri/binaries/sing-box-aarch64-apple-darwin
```

`SPLITROUTE_HELPER_SOCKET` переопределяет путь сокета; в debug по умолчанию —
`$TMPDIR/splitroute-helper.sock`. Состояние приложения —
`<config>/splitroute/state.json` (права `0600`).

### 4.5. Подпись и helper в dev

- Локальная сборка приложения: ad-hoc подпись `codesign --force --deep --sign - <app>`; для Tauri это делается в конфиге бандла.
- **Helper как LaunchDaemon требует настоящую подпись.** На macOS 15.8.2 AMFI отвергает ad-hoc/linker-подпись (`has no CMS blob`) и убивает процесс. Нужен сертификат Apple Development/Developer ID; установщик проверяет подпись до копирования и ad-hoc не принимает.
- Dev-режим helper'а: отдельный процесс под `sudo`, сокет в `/tmp/splitroute-helper.sock`, без установки LaunchDaemon.
- Production-режим: LaunchDaemon в `/Library/LaunchDaemons`, сокет в `/var/run/splitroute/`, права `0600` для пользователя-владельца. Установка/удаление — только через UI и с явным подтверждением.
- Если helper не устанавливается: проверить подпись (`codesign -dv`), путь plist, `launchctl print system/<label>`, логи `log show --predicate 'process == "splitroute-helper"' --last 5m`.

---

## 5. Поднятие среды: CachyOS / Arch Linux

### 5.1. Системные зависимости Tauri 2

```bash
sudo pacman -Syu --needed base-devel curl wget file openssl git \
  webkit2gtk-4.1 gtk3 libsoup3 libappindicator-gtk3 librsvg patchelf \
  nodejs npm
```

### 5.2. Rust

```bash
sudo pacman -S --needed rustup
rustup default stable
rustc --version
```

Не используем `pacman -S rust`, если нужен актуальный toolchain: через rustup управлять версиями проще.

### 5.3. Node и pnpm

```bash
node --version                  # 22+
corepack enable
corepack prepare pnpm@latest --activate
```

Если corepack отсутствует: `sudo npm install -g corepack` или установить Node через `fnm`.

### 5.4. Запуск и сборка

```bash
cd app
pnpm install
./scripts/fetch-core.sh
sudo ./scripts/dev-helper.sh    # dev-режим, без установки systemd-юнита
pnpm tauri dev
```

Сборка пакетов:

```bash
./scripts/package.sh            # deb/rpm/appimage + sidecar helper'а; только на Linux
```

- `scripts/package.sh` сам собирает sidecar helper'а (`scripts/build-helper-sidecar.sh`)
  и выбирает цели по ОС. Перед ним нужен `./scripts/fetch-core.sh`.
- `externalBin` живёт только в `tauri.linux.conf.json`/`tauri.windows.conf.json`:
  `tauri-build` ищет sidecar на `cargo check`, а бинари в `.gitignore` — в базовом
  конфиге чистый клон не собрался бы.
- Для запуска AppImage может понадобиться `fuse2` (`sudo pacman -S fuse2`) или `--appimage-extract-and-run`.
- Для сборки AppImage — `linuxdeploy` и `patchelf`; если bundler ругается, проверить `pnpm tauri info`.

### 5.5. Права на Linux (production)

- Helper — systemd-юнит `splitroute-helper.service` под root (`User=root`,
  `RuntimeDirectory=splitroute` `0711`, `UMask=0077`, `Restart=on-failure`),
  сокет `/run/splitroute/helper.sock`, доступ по uid (`--allow-uid`).
- Установка юнита — через polkit (`pkexec`) из UI с подтверждением; ручное
  удаление — `systemctl disable --now splitroute-helper`.
- TUN требует `/dev/net/tun` и прав; проверка: `ls -l /dev/net/tun`.
- Для ручной отладки допустим запуск helper'а под `sudo`, но UI не должен хранить root-пароль.

---

## 6. Поднятие среды: Ubuntu и Fedora

### Ubuntu 22.04 / 24.04

```bash
sudo apt update
sudo apt install -y build-essential curl wget file git pkg-config \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libxdo-dev libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env" && rustup default stable
# Node 22: через fnm/nvm или NodeSource; затем
corepack enable && corepack prepare pnpm@latest --activate
```

Если `libwebkit2gtk-4.1-dev` недоступен (старый релиз) — обновиться до 24.04 или использовать официальный репозиторий Tauri; не подменять на 4.0 молча.

### Fedora 40+

```bash
sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file git \
  libappindicator-gtk3-devel librsvg2-devel gcc gcc-c++ make
sudo dnf group install -y "C Development Tools and Libraries"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env" && rustup default stable
corepack enable && corepack prepare pnpm@latest --activate
```

---

## 7. Windows (сборка и проверка)

Разработка ведётся на macOS/CachyOS, но сборку и smoke-тесты Windows нужно уметь воспроизводить (VM или CI).

- Visual Studio Build Tools с workload «Desktop development with C++», Windows SDK.
- Rust MSVC: `rustup default stable-msvc`, target `x86_64-pc-windows-msvc`.
- WebView2 Runtime (в Windows 11 встроен; для Windows 10 — установить).
- `wintun.dll` кладётся рядом с `sing-box.exe` в бандле; версия фиксируется вместе с ядром. Официальная подписанная библиотека — `src-tauri/binaries/wintun.dll` (`SPLITROUTE_WINTUN_DLL` переопределяет путь), скачивается `./scripts/fetch-wintun.sh` (wintun 0.14.1, проверка SHA256). В пакет попадает через `bundle.resources` в `tauri.windows.conf.json`: на Windows `$RESOURCE` — это каталог экзешника, поэтому `"binaries/wintun.dll": "wintun.dll"` кладёт файл куда нужно.
- Dev-helper: `splitroute-helper.exe console --pipe ... --allow-sid <SID>`, консоль от администратора, служба не регистрируется. Production — служба `splitroute-helper-service` под SYSTEM и named pipe `\\.\pipe\splitroute-helper` с ACL на SID пользователя; helper стартует как `splitroute-helper.exe service --pipe ... --allow-sid <SID>`. Без администратора TUN не поднимется, но UI и обмен по pipe работают.
- Установка/удаление службы — установщик приложения через UAC (`Start-Process -Verb RunAs`), не сокет: helper не умеет `install`/`uninstall`.
- `sc.exe create` требует `binPath` одним экранированным аргументом, иначе `service` и флаги отвергаются как неизвестные параметры.
- Кросс-компиляция NSIS с macOS/Linux не поддерживается штатно — использовать Windows-раннер или VM. Полный `cargo check -p splitroute --target x86_64-pc-windows-msvc` на macOS тоже не проходит (`ring` без MSVC-заголовков); проверяйте `splitroute-helper` и модули установщика.

---

## 8. Ядро sing-box

- Версия и sha256 — в `scripts/core-version.txt`; обновление — отдельный коммит `chore: обновлён sing-box до X.Y.Z` с прогоном golden-тестов.
- `scripts/fetch-core.sh` определяет target triple, скачивает архив с официальных релизов, проверяет контрольную сумму, распаковывает в `src-tauri/binaries/sing-box-<triple>` и делает `chmod +x`.
- Tauri sidecar: `externalBin: ["binaries/sing-box"]`, имя файла с triple.
- Перед запуском helper обязан выполнить `sing-box check -c <config>`; невалидный конфиг не запускается.
- Конфиг пишется только в root-каталог helper'а, права `0600`, удаляется при остановке.
- Inbounds sing-box слушают только `127.0.0.1` (mixed-inbound), TUN — штатный интерфейс.
- Логи ядра: helper читает stdout/stderr, отдаёт приложению по подписке; в файл — с ротацией.

---

## 9. Команды

Из корня `app/`:

```bash
pnpm install                 # зависимости фронтенда
pnpm dev                     # только Vite, без Tauri
pnpm tauri dev               # приложение в dev-режиме
pnpm tauri build             # сборка бандла
pnpm typecheck               # tsc --noEmit
pnpm lint                    # eslint
pnpm test                    # vitest

cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p splitroute-helper

./scripts/fetch-core.sh
./scripts/build-helper-sidecar.sh   # helper под именем <triple> для externalBin
./scripts/package.sh                # пакет текущей ОС (deb/rpm/appimage, nsis, app/dmg)
./scripts/check-config.sh path/to/config.json
```

Кросс-проверка кода под другие ОС (без целевой машины): полный `cargo check -p
splitroute --target ...` на macOS невозможен (ring/gio требуют sysroot и
MSVC-заголовки), но `cargo check`/`clippy` для `splitroute-helper` и
платформенных модулей установщика работает — так проверяются windows/linux
ветки, пока нет Linux-машины и Windows VM.

Правило: перед коммитом — `pnpm typecheck && pnpm lint && cargo fmt --all --check && cargo clippy ... && cargo test --workspace` (или соответствующий CI-job).

---

## 10. Архитектурные правила

1. **Разделение ответственности.** UI → Tauri commands → Rust core → helper → sing-box. UI не формирует конфиг, не знает путей ядра и не работает с root.
2. **Минимальная привилегированная поверхность.** Helper принимает только `install`, `uninstall`, `start`, `stop`, `status`. Никаких `exec`, произвольных путей и shell-команд.
3. **Аутентификация IPC.** Unix socket: `0600`, владелец — пользователь, helper проверяет peer uid через `SO_PEERCRED`/`getpeereid`. Windows: named pipe с ACL на SID пользователя, проверка клиента. Сообщения — JSON с версией протокола и лимитом размера.
4. **Конфиг — данные, не код.** Генератор возвращает `serde_json::Value`; helper не исполняет пользовательские строки. Все внешние строки (имена профилей, пути) экранируются при логировании.
5. **Секреты и HWID.** URL подписки, UUID и приватные параметры не попадают в git, логи и диагностику. В пользовательском конфиге — с правами `0600`. HWID — 16 hex из CSPRNG, генерируется один раз при первом запуске, хранится локально, не выводится из серийников/MAC и может быть изменён пользователем вручную. HWID не считается секретом, но не должен попадать в публичные отчёты.
6. **Чистые функции и тесты.** `parse_subscription(&str) -> Result<Vec<Profile>>` и `build_config(&Settings, &Profile, &[AppRule]) -> Result<Value>` тестируются golden-файлами.
7. **Ошибки.** В библиотеках — `thiserror`, на границе Tauri commands — `anyhow` с контекстом; пользователю — типизированный код ошибки + русский текст.
8. **Состояние.** Единый источник истины в Rust; UI — проекция через события. Никаких двух независимых «подключено/отключено».
9. **Платформенный код.** `#[cfg(target_os = ...)]` изолирован в `privileged/` и `apps/`, не размазан по бизнес-логике.
10. **Никакой телеметрии.** Внешние запросы только: подписка, проверка задержки, прокси-соединение.

---

## 11. Протокол helper'а (черновик)

```json
{ "protocol": 1, "id": "uuid", "command": "start", "payload": { "config": { } } }
{ "protocol": 1, "id": "uuid", "ok": true, "payload": { "pid": 1234, "state": "running" } }
{ "protocol": 1, "id": "uuid", "ok": false, "error": { "code": "CONFIG_INVALID", "message": "..." } }
```

- `status` возвращает состояние, pid, версию ядра, время старта.
- `stop` идемпотентен.
- Таймаут на ответ — 10 с; helper не блокируется на одном клиенте.
- Версия протокола проверяется; несовпадение → helper считается устаревшим и предлагается переустановка.

---

## 12. Соглашения проекта

### Код

- Комментарии — **на русском**. Имена переменных, функций, файлов, типов — английские.
- Комментарии только там, где логика неочевидна; не дублировать код.
- Форматирование: `cargo fmt`, Prettier. Линт: `clippy -D warnings`, ESLint без отключений без причины.
- Публичные функции ядра — с короткой документацией `///` на русском.
- Не добавлять зависимости «на будущее»; каждая новая — с обоснованием в PR/коммите.

### Git

- Ветка по умолчанию — `main`. Рабочие ветки: `feat/...`, `fix/...`, `chore/...`.
- Коммиты: `<type>: <описание на русском>`, тип — английский.
  - `feat: добавлен выбор приложений в allowlist`
  - `fix: исправлено падение парсера без outbound proxy`
  - `docs: обновлён раздел про macOS NetworkExtension`
  - `chore: обновлён sing-box до 1.12.x`
  - `test: добавлены golden-тесты генератора конфига`
- Заголовок до 72 символов, без точки в конце; тело — по желанию, на русском.
- Один коммит — одна логическая задача. Не смешивать рефакторинг и фичу.
- Секреты, бинарники ядра и артефакты сборки в git не попадают.
- Коммит создаётся по завершении задачи, если пользователь не попросил иначе.
- **Порядок внутри этапа:** сначала коммит(ы) с кодом и тестами (`feat:`/`fix:`/`test:`/`refactor:`), затем отдельный коммит с изменёнными документами (`docs:`). Не смешивать код и документацию в одном коммите.
- Этап не считается завершённым, пока не обновлён `NEXT_SESSION.md` (см. §17).

### Документация

- Изменение поведения → правка PRD.md в том же коммите с документацией этапа.
- Новый платформенный факт/ограничение → запись в docs/research.md с датой и ссылкой.
- AGENTS.md обновляется при изменении команд, структуры или среды.
- `TASKS.md` — статусы задач и решения по ним; `NEXT_SESSION.md` — handoff для следующей сессии.

---

## 13. Тестирование и Definition of Done

### Автотесты

- Парсер подписки: валидный JSON, пустой массив, отсутствующий `proxy`, битый JSON, HTTP-ошибки (мокать клиент).
- Генератор конфига: golden-файлы для allowlist, direct-правил, IPv6 on/off, DNS-режимов.
- Протокол helper'а: сериализация/десериализация, неизвестная команда, несовпадение версии.
- UI: ключевые экраны и состояния (Vitest + Testing Library), без скриншотных тестов в MVP.

### Ручная матрица (минимум)

| Проверка | macOS | CachyOS | Ubuntu | Windows |
|---|---|---|---|---|
| Подключение/отключение | ✓ | ✓ | ✓ | ✓ |
| Выбранное приложение идёт через прокси | ✓ | ✓ | ✓ | ✓ |
| Невыбранное приложение идёт напрямую | ✓ | ✓ | ✓ | ✓ |
| Браузер (Chrome/Safari/Firefox) | ✓ | ✓ | ✓ | ✓ |
| Telegram | ✓ | ✓ | ✓ | ✓ |
| CLI-утилита из терминала | ✓ | ✓ | ✓ | ✓ |
| DNS без утечек | ✓ | ✓ | ✓ | ✓ |
| Сон/пробуждение, смена сети | ✓ | ✓ | ✓ | ✓ |
| Выход без зависших TUN/процессов | ✓ | ✓ | ✓ | ✓ |
| Ошибка подписки и восстановление | ✓ | ✓ | ✓ | ✓ |

Проверка «невыбранное напрямую»: сравнить внешний IP процесса с IP прокси (например, `curl ifconfig.me` внутри выбранного и невыбранного приложения/процесса).

### Definition of Done

- [ ] Поведение соответствует PRD; при изменении — PRD обновлён.
- [ ] Есть тесты (или объяснение, почему их нет) и они зелёные.
- [ ] `pnpm typecheck`, `pnpm lint`, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` проходят.
- [ ] Проверено на целевой ОС вручную по релевантным пунктам матрицы.
- [ ] Логи не содержат секретов; ошибки понятны пользователю.
- [ ] Обновлены AGENTS.md/research.md, если менялись команды, среда или платформенные факты.
- [ ] `TASKS.md` отражает фактический статус задач этапа.
- [ ] `NEXT_SESSION.md` содержит готовый промпт для следующей сессии.
- [ ] Код и документация закоммичены отдельными коммитами (сначала код, затем docs).
- [ ] Коммит оформлен по соглашениям.

---

## 14. Правила для AI-агентов

1. **Не начинай с кода, если меняется продукт.** Сначала предложи правку PRD, дождись согласования.
2. **Спайк перед платформенным риском.** Для macOS process-правил, DNS, QUIC, helper'ов — сначала минимальный эксперимент, результат в docs/research.md, потом реализация.
3. **Не добавляй зависимости и не меняй архитектуру молча.** Обоснуй, покажи альтернативы, выбери минимальную.
4. **Не трогай сторонние бинарники.** sing-box только скачивается pinned-версией, без патчей в репозитории.
5. **Не коммить секреты.** Проверяй diff на URL подписки, HWID, UUID, токены.
6. **Не выполняй разрушительные команды** (`rm -rf`, `git reset --hard`, удаление сервисов, `systemctl disable`) без явного запроса; для установки helper'а — только через предусмотренный интерфейс.
7. **Не оставляй грязную среду.** Запущенные dev-серверы, helper'ы и TUN-интерфейсы останавливаются после проверки.
8. **Следуй комментариям и коммитам на русском**; идентификаторы — английские.
9. **Обновляй документы в том же изменении**, а не отдельным «когда-нибудь».
10. **Если факт не проверен — помечай как гипотезу** и не выдавай за требование.
11. **Субагенты не коммитят, не правят PRD/AGENTS/TASKS/NEXT_SESSION и не запускают других субагентов.** Оркестратор собирает результат, проверяет, коммитит код и отдельно — документацию.
12. **Перед делегированием зафиксируй контракт:** типы, сигнатуры, формат файлов и критерии приёмки. Задача субагенту должна быть самодостаточной: файлы, ограничения, команды проверки.
13. **Разделяй владение файлами.** Два субагента не должны одновременно править один файл; пересечения — только через оркестратора.

---

## 15. Известные платформенные грабли

- **macOS 27+**: process-правила могут не работать у не-Apple-подписанных бинарей (sysctl `pcblist` → 48 байт). См. PRD R1. На macOS 15.8 работает; проверять на актуальных версиях перед релизом.
- **macOS**: helper как LaunchDaemon требует настоящей подписи (Apple Development/Developer ID); ad-hoc/linker-подпись AMFI убивает (`has no CMS blob`) — проверено на macOS 15.8.2 (2026-11-12). Для обычного приложения ad-hoc годится. NetworkExtension требует entitlement и Apple Developer Program.
- **macOS 15 + sing-box 1.14**: DNS-правила по процессу для TUN не матчатся — `dns/tls[remote]` остаётся unreferenced, DNS выбранного приложения уходит в локальный resolver. `hijack-dns` и `sniff` это не исправляют; remote DoT через прокси сам по себе работает. Решение — PRD §6.4: два режима, `proxy` (весь DNS через прокси, по умолчанию) и `system` (DNS ОС, с предупреждением); `process` выводится из UI. Подробности — `docs/research.md` §9 и `docs/stage-2-report.md`.
- **Windows**: TUN требует прав администратора, `wintun.dll` должен быть рядом с ядром; Defender/Firewall может блокировать первый запуск; UDP/QUIC матчинг нестабилен.
- **Linux**: `systemd-resolved` конфликтует с DNS в туннеле; трей требует `libappindicator` (на Wayland возможны проблемы); AppImage требует FUSE 2 или распаковки; Flatpak/Snap-приложения имеют sandbox-пути.
- **Все ОС**: другие VPN/Docker создают TUN-интерфейсы и маршруты — нужен детект конфликта; смена сети и сон/пробуждение ломают существующий туннель, нужен reconnect.
- **Дочерние процессы**: правило матчится по процессу-владельцу сокета, а не по «терминалу»; для CLI используем конкретные пути/каталоги.
- **GPL-3.0**: проект под GPL-3.0; при поставке sing-box и заимствовании кода из Clash Verge Rev/FlClash нужны `LICENSE`, `THIRD_PARTY_NOTICES` и предложение исходников.

---

## 16. Troubleshooting среды

| Симптом | Что проверить |
|---|---|
| `webkit2gtk` не найден на Linux | установлен ли `webkit2gtk-4.1` (не 4.0), `pkg-config --modversion webkit2gtk-4.1` |
| Белое/чёрное окно WebKit | `WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev` |
| Нет иконки в трее Linux | `libappindicator-gtk3`/`libayatana-appindicator3`, расширение AppIndicator в DE |
| `pnpm: command not found` | `corepack enable && corepack prepare pnpm@latest --activate` |
| Rust-сборка падает на OpenSSL | использовать rustls-фичи, проверить `openssl`/`pkg-config` |
| Helper не отвечает | сокет существует и права `0600`, процесс запущен, версия протокола совпадает, логи `tracing` |
| TUN не создаётся | права root/CAP_NET_ADMIN, `/dev/net/tun`, не занят ли TUN другим VPN |
| macOS: process-правило не срабатывает | проверить версию macOS (риск R1), имя процесса и путь через `ps`/`lsof`, лог матчинга |
| Windows: нет трафика через TUN | `wintun.dll` рядом с ядром, запущен ли helper от SYSTEM, исключения Firewall |
| Сборка AppImage падает | `fuse2`, `linuxdeploy`, `patchelf`, `pnpm tauri info` |

---

## 17. Процесс этапов и NEXT_SESSION.md

Работаем небольшими этапами из `TASKS.md`. Один этап — одна законченная порция результата, которую можно проверить.

Порядок внутри этапа:

1. **Старт.** Прочитать `PRD.md`, `AGENTS.md`, `TASKS.md`, `NEXT_SESSION.md`, `docs/research.md`. `NEXT_SESSION.md` — основной вход: там состояние проекта и промпт текущего этапа.
2. **Код.** Выполнить задачи этапа, проверить тестами и линтом.
3. **Коммит кода.** Отдельный коммит(ы) с кодом и тестами.
4. **Документы.** Обновить PRD/AGENTS/research/TASKS и `NEXT_SESSION.md`. Отдельный коммит `docs: ...`.
5. **Handoff.** Перезаписать `NEXT_SESSION.md`: состояние, что сделано, что не сделано, следующий этап, готовый промпт для новой сессии, известные проблемы.
6. **Стоп.** Не начинать следующий этап в той же сессии, если пользователь не попросил.

Требования к `NEXT_SESSION.md`:

- Первый блок — «Состояние»: этап, дата, ветка, последний коммит, что работает, что сломано.
- Второй блок — «Промпт следующей сессии»: самодостаточный текст, который можно вставить в новую сессию без чтения истории чата.
- В промпте: цель этапа, список задач с ID из `TASKS.md`, файлы/модули, ограничения, команды проверки, формат отчёта.
- Промпт не содержит секретов (URL подписки, HWID, UUID).
- Файл всегда отражает последний завершённый этап; история этапов — в git, а не в этом файле.

---

## 18. Оркестрация: `/goal` и субагенты

Режим с основным агентом-оркестратором и субагентами поддерживается: `/goal <objective>` запускает автономный цикл по цели с лимитом шагов, а основной агент делегирует задачи через инструмент `agent`. Субагенты работают параллельно, в изолированном контексте, и **не могут запускать своих субагентов** (глубина — один уровень).

### Роли

- **Оркестратор (основной агент).** Держит цель, читает `TASKS.md`, дробит работу, пишет самодостаточные промпты, собирает результаты, запускает проверки, коммитит, обновляет документы и `NEXT_SESSION.md`.
- **`rust-core`.** Rust-ядро: подписка, модели, генератор конфига, состояние, Tauri commands.
- **`react-ui`.** React/TS: экраны, компоненты, состояния, i18n, обёртки API.
- **`platform-native`.** macOS/Windows/Linux: helper, сокеты/пайпы, установка сервисов, TUN-обвязка.
- **`verifier`.** Read-only проверка: тесты, линт, ревью diff, поиск секретов и расхождений с PRD. Ничего не правит.

### Правила параллельной работы

1. **Контракты раньше кода.** Сначала фиксируются общие типы (`crates/protocol`), сигнатуры команд и формат данных. Только после этого задачи уходят субагентам.
2. **Один файл — один владелец.** Задачи нарезаются по файлам/модулям; пересечения возможны только через оркестратора.
3. **Промпт субагенту самодостаточен.** Указываются: цель, конкретные файлы, существующие контракты, что нельзя менять, команды проверки, формат ответа. Субагент не видит историю чата и не может задать вопрос.
4. **Максимум 3–4 параллельных субагента.** Больше — сложнее интегрировать и проверять.
5. **Субагенты не коммитят.** Оркестратор проверяет diff, запускает проверки и коммитит сам.
6. **Изоляция при риске конфликтов.** Если две задачи неизбежно трогают общие файлы — worktree на задачу или последовательное выполнение.
7. **Интеграция обязательна.** После параллельной работы оркестратор прогоняет сборку, тесты и линт; «каждый сделал своё» не считается результатом.
8. **Верификация отдельно.** `verifier` получает diff и критерии приёмки; его отчёт прикладывается к коммиту или этапу.
9. **Цель формулируется проверяемо.** `/goal` должен ссылаться на этап и задачи из `TASKS.md`, а не на «сделать хорошо».
10. **Без бесконечного цикла.** Если субагент не уложился в задачу — оркестратор уменьшает объём, а не повторяет тот же промпт.

Описания субагентов лежат в `agents/` — это источник истины в обычном Markdown, не привязанный к харнессу. Для Command Code выполни `./scripts/sync-agents.sh`: он создаст локальную проекцию в `.commandcode/agents/` (в git не попадает). Для другого харнесса укажи путь явно, например `./scripts/sync-agents.sh .claude/agents`, либо используй файлы ролей как системные промпты. Сессию запускаем из `app/`, чтобы подхватился `AGENTS.md`. Пример цели: `/goal Выполни этап 1 из TASKS.md согласно AGENTS.md`. Промпт для новой сессии всегда берём из `NEXT_SESSION.md`.

### Как ставить цель, чтобы не упереться в лимит

1. **Одна цель — один небольшой проверяемый кусок.** Не «сделай этап 1», а «выполни S1-T1…S1-T3 и остановись». Этап 1 из 12 задач делится на 3–4 цели.
2. **Границы в тексте цели.** Укажи ID задач, каталоги/файлы, критерий готовности, что нельзя трогать и что после завершения нужно закоммитить и обновить `NEXT_SESSION.md`.
3. **Явный стоп.** Формулируй «остановись после коммита и отчёта», а не «продолжай, пока не будет идеально». Иначе цикл тратит шаги на полировку.
4. **Больше делегирования, меньше шагов оркестратора.** Параллельные субагенты (3–4) выполняют работу в своих циклах; оркестратор тратит шаги только на постановку, сборку и проверку.
5. **Фоновые субагенты.** Для долгих задач — `run_in_background` и `agent_output` с `wait`, чтобы не опрашивать статус в цикле.
6. **Коммит после каждой подцели.** Тогда при исчерпании лимита работа не теряется: новая сессия продолжает с `NEXT_SESSION.md`, а не начинает заново.
7. **Контроль без расхода шагов.** `/goal status` (или bare `/goal`) — прогресс, `/goal clear` — мягкая остановка после текущего шага.
8. **Если задача объективно длинная** — headless-режим `cmd -p "…" --max-turns N` (по документации верхнего предела нет, выход с кодом 8 при достижении лимита) либо цепочка отдельных целей. Интерактивный `/goal` отдельного флага для лимита не документирует.
9. **Не переиспользуй проваленную цель.** Если лимит близко, а результата нет, — уменьши объём и переформулируй, а не повторяй тот же промпт.
