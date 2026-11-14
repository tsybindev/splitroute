# TASKS.md — реестр этапов и задач

Единый источник истины по объёму работ. Оркестратор читает этот файл, дробит задачи между субагентами и обновляет статусы в коммите с документацией этапа.

## Правила ведения

- Статусы: `todo`, `in-progress`, `blocked`, `done`.
- ID задачи не меняются и не переиспользуются: `S<этап>-T<номер>`.
- Выполненные задачи не удаляются — меняется статус и добавляется ссылка на коммит/отчёт.
- Новая задача, обнаруженная по ходу этапа, добавляется с новым ID и попадает в ближайший этап или в `backlog`.
- Задача считается выполненной только при выполнении критериев приёмки; «код написан» — не критерий.
- Секреты (URL подписки, HWID, UUID, реальные адреса серверов) в задачах, фикстурах и отчётах не хранятся.

---

## Этап 0 — документация и среда (done)

| ID | Задача | Статус |
|---|---|---|
| S0-T1 | PRD.md: продукт, scope MVP, UX, риски, roadmap | done |
| S0-T2 | AGENTS.md: стек, архитектура, среда macOS/CachyOS/Ubuntu/Fedora, соглашения | done |
| S0-T3 | docs/research.md: проверенные факты и ссылки | done |
| S0-T4 | Git-репозиторий в `app/`, `.gitignore`, README | done |
| S0-T5 | Решения: название SplitRoute, лицензия GPL-3.0, HWID, kill switch | done |
| S0-T6 | LICENSE (GPL-3.0), TASKS.md, NEXT_SESSION.md, проектные субагенты | done |

---

## Этап 1 — spike: TUN + process-правила (done с оговорками)

Цель: доказать или опровергнуть, что выбранная архитектура (sing-box sidecar + TUN + process-правила + root-helper) работает на целевых ОС, и зафиксировать ограничения. Продакшн-код на этом этапе не пишется: только воспроизводимые эксперименты в `spike/` и отчёт.

| ID | Задача | Владелец | Зависит от | Файлы | Критерий приёмки |
|---|---|---|---|---|---|
| S1-T1 | Скачать pinned sing-box, скрипт с SHA256, `spike/fetch-core.sh` | platform-native | — | `scripts/core-version.txt`, `scripts/fetch-core.sh` | Бинарь скачивается и запускается, версия сверена, хэш проверяется | done — `3a7bb98` |
| S1-T2 | Санитизированная фикстура подписки (без реальных URL/HWID/UUID/адресов) | rust-core | — | `spike/fixtures/subscription.json` | Фикстура валидна, не содержит секретов, покрывает 2 профиля и лишние outbound'ы | done — `3a7bb98` |
| S1-T3 | Скрипт конвертации Xray-профиля в sing-box outbound + `sing-box check` | rust-core | S1-T1, S1-T2 | `spike/convert.py`, `spike/configs/` | Для обоих профилей конфиг проходит `sing-box check`, поля Reality/SNI/flow совпадают с исходником | done — `3a7bb98` |
| S1-T4 | macOS 15.x: TUN + process-правила для Chrome, Telegram, CLI (`curl`) | platform-native | S1-T1, S1-T3 | `spike/macos/README.md`, логи | process-правила (вкл. helper'ы Chrome) подтверждены без root, см. `spike/macos/README.md`; TUN и сравнение внешнего IP — **не проверено** (нет root/реальной подписки) |
| S1-T5 | macOS 26/27: проверка ограничения R1 (process-правила) | platform-native | S1-T4 | `docs/spikes/stage-1.md` | **не проверено**: macOS 26/27 недоступна, зафиксировано в `spike/macos/README.md` |
| S1-T6 | Linux CachyOS: TUN + process-правила | platform-native | S1-T1, S1-T3 | `spike/linux/README.md`, логи | **не проверено**: машины с CachyOS нет, Docker-демон не запущен |
| S1-T7 | Windows (VM): TUN + `wintun.dll` + process-правила | platform-native | S1-T1, S1-T3 | `spike/windows/README.md`, логи | **не проверено**: Windows VM/UTM/Parallels/VirtualBox/qemu отсутствуют |
| S1-T8 | DNS-политика: разделение по процессу, `systemd-resolved`, Private Relay | platform-native | S1-T4, S1-T6 | `docs/spikes/stage-1.md` | in-progress — разделение DNS по процессу подтверждено без TUN (`spike/macos/dns_rules_test.py`); утечки через TUN, `systemd-resolved` и Private Relay **не проверены** |
| S1-T9 | QUIC/UDP: проходит ли трафик выбранного приложения, нужна ли блокировка | platform-native | S1-T4 | `docs/spikes/stage-1.md` | in-progress — UDP и process-правила для UDP подтверждены (`spike/macos/udp_rules_test.py`); QUIC поверх TUN не проверен, предварительное решение в `spike/macos/README.md` |
| S1-T10 | Прототип helper: macOS LaunchDaemon и Linux systemd, сокет и права | platform-native | S1-T4, S1-T6 | `spike/helper/` | done — прототип: сокет 0600, uid пира, только start/stop/status, конфиг через `sing-box check`, 6 тестов (`spike/helper/README.md`); установка сервисов и подпись — этап 3 |
| S1-T11 | Отчёт этапа: go/no-go по платформам, риски, решение по macOS | оркестратор | S1-T4…S1-T10 | `docs/spikes/stage-1.md` | done — отчёт: методика, версии, результаты, go/no-go, обновлённые риски (`docs/spikes/stage-1.md`) |
| S1-T12 | Handoff этапа 2 | оркестратор | S1-T11 | `NEXT_SESSION.md`, `TASKS.md` | done — `NEXT_SESSION.md` содержит самодостаточный промпт этапа 2, статусы обновлены |

---

## Этап 2 — ядро MVP (done: G2.1–G2.6; S2-T9 blocked — нет CachyOS)

| ID | Задача | Владелец | Критерий приёмки |
|---|---|---|---|
| S2-T1 | Tauri 2 скелет, workspace, CI-команды | оркестратор | `pnpm tauri dev` запускается на macOS и CachyOS — **macOS 15.8.2: done**, `pnpm tauri dev` стартует (Vite + окно); CachyOS **не проверено** (нет машины) |
| S2-T2 | `crates/protocol`: типы IPC и общие модели | rust-core | done — Request/Response/Command/ErrorCode, HelperStatus, Profile, AppRule, Settings; 14 тестов, clippy чистый |
| S2-T3 | Парсер подписки + фикстуры + тесты | rust-core | done — `src-tauri/src/subscription/`: VLESS+Reality/TLS+TCP → `Profile`, пропуски с причинами; тесты на валидный/пустой/нет proxy/битый JSON/неподдерживаемый транспорт + фикстура `spike/fixtures/subscription.json` |
| S2-T4 | Генератор sing-box конфига + golden-тесты | rust-core | done — `src-tauri/src/config/`: TUN, DNS по процессу/системный, allowlist, IPv6 on/off, блок IPv6 для выбранных; golden-файлы + интеграционный `sing-box check` |
| S2-T5 | Клиент helper'а (mock + реальный), установка/удаление | platform-native | done — `src-tauri/src/privileged/`: трейт `HelperClient`, Unix-транспорт (JSON-строки, лимит, таймаут, версия), mock, типизированные ошибки; `Installer`-заглушка (установка — этап 3); tests 9+4+1 |
| S2-T6 | Экраны: Главная, Локации, Приложения, Настройки, Логи | react-ui | done — экраны по PRD §7, i18n ru, все состояния и ошибки; обёртки в `src/shared/api`, состояние в Zustand; тесты 24 (Vitest) |
| S2-T7 | Трей, автозапуск, single instance | react-ui | done — меню трея (статус, подключить/отключить, открыть, выход), `tauri-plugin-autostart`, `tauri-plugin-single-instance`, закрытие в трей; single instance проверен headless; трей визуально **не проверен**, автозапуск в системе не регистрировался |
| S2-T8 | Сквозной сценарий на macOS: подписка → локация → подключение → allowlist | оркестратор | done — отчёт `docs/stage-2-report.md`: реальная подписка (Happ UA → 2 профиля VLESS+Reality/TCP), реальный TUN, выбранный `curl` через прокси, невыбранный `python3` напрямую, выключение правила, очистка TUN/маршрутов; ошибки подписки и UI-состояния покрыты тестами. **DNS-утечка выявлена** (см. S2-T10), трей визуально **не проверен** (нет UI-автоматизации) |
| S2-T9 | Сквозной сценарий на CachyOS | platform-native | **blocked** — машины с CachyOS нет; перенесено в этап 3 (S3-T7) |
| S2-T10 | DNS-политика: два пользовательских режима `proxy` и `system` | rust-core + react-ui | done — `DnsMode::{Proxy,System}`, старое `process` читается как `proxy` (тесты serde/миграции); `build_dns`/`build_route`: proxy — remote DoT + local, локальные имена (`.local`, `home.arpa`) → local, `final=remote`, первым `hijack-dns`; system — только local, без правил и hijack; golden обновлены, `sing-box check` зелёный; connect логирует режим и для system пишет Warn; UI — только два варианта с подсказками, бейдж DNS. Реальный TUN-тест (2026-11-12, реальная подписка + dev-helper под root): `proxy` — DNS выбранного и невыбранного процесса → IP прокси, `remote` referenced, `localhost` резолвится; `system` — DNS выбранного процесса → домашний IP (ожидаемая утечка, UI предупреждает); `stop` очищает TUN/маршрут |

---

## Этап 3 — helper'ы и упаковка (in-progress: S3-T1/T3/T4/T5/T6/T8 done; S3-T2 blocked — подпись; S3-T7 blocked — нет Linux/Windows)

| ID | Задача | Владелец | Критерий приёмки |
|---|---|---|---|
| S3-T1 | Rust-helper: сокет/пайп, команды, проверка uid/SID, `sing-box check` | platform-native | done — `crates/helper`: Unix-сокет `0600`, uid пира (`getpeereid`/`SO_PEERCRED`), `status/start/stop`, `install`/`uninstall` по сокету запрещены, лимит 1 МиБ и версия протокола, конфиг `0600` + `sing-box check`, остановка ядра по SIGTERM, второй экземпляр не трогает конфиг; 19 lib + 4 args + 11 интеграционных тестов, реальный sing-box 1.14.4 без root, round-trip клиента; коммит `d7af3ba` |
| S3-T2 | macOS: LaunchDaemon, установка/удаление, подпись helper'а | platform-native | **blocked** — код и 12 тестов готовы: `MacosInstaller`, plist `com.splitroute.helper`, пути `/Library/{LaunchDaemons,PrivilegedHelperTools}`, `/var/run/splitroute` `0711`, сокет `0600`, установка/удаление через `osascript` с системным запросом прав. Реальная попытка установки: файлы и plist встали, но macOS 15.8.2 AMFI убила ad-hoc/linker-подпись helper'а (`has no CMS blob`), сервис не загрузился, сокета нет; после проверки всё удалено. `security find-identity -v -p codesigning` → 0; нужен Apple Development/Developer ID. Установщик проверяет подпись до копирования (`d9da641`). Чек-лист — `docs/stage-3-report.md` §6 |
| S3-T3 | Linux: systemd + polkit, `.deb`/`.rpm`/AppImage | platform-native | **done по коду** — юнит `splitroute-helper.service` (`User=root`, `RuntimeDirectory=splitroute`, `RuntimeDirectoryMode=0711`, `UMask=0077`, `Restart=on-failure`), раскладка в `/usr/lib/splitroute`, сокет `/run/splitroute/helper.sock`, установка/удаление через `pkexec` (отмена = код 126, нет прав = 127), `--allow-uid` из `/proc/self/status`; 13 тестов (юнит, ExecStart, скрипты, `sh -n`, пути, uid, экранирование). Клиент в release ищет тот же сокет. Пакеты: `tauri.linux.conf.json` (deb/rpm/appimage + `externalBin`), `scripts/package.sh`. **Не проверено:** реальная установка/systemctl/polkit и сборка `.deb`/`.rpm`/AppImage (нет Linux-машины; кросс-`cargo check` падает на gio-sys). Коммиты `be1d918`, `684e6e5`, `276f218` |
| S3-T4 | Windows: служба под SYSTEM, named pipe ACL, `wintun.dll` | platform-native | **done по коду** — helper под Windows: named pipe `\\.\pipe\splitroute-helper` с SDDL-ACL только на SID пользователя, SYSTEM и Administrators + сверка SID процесса-клиента; служба `splitroute-helper-service` под SYSTEM (`StartServiceCtrlDispatcherW`, `SERVICE_CONTROL_STOP`, статусы). Установщик: `C:\Program Files\SplitRoute` (`wintun.dll` рядом с `sing-box.exe`), runtime `C:\ProgramData\SplitRoute` с `icacls`, `sc.exe create ... obj= LocalSystem`, UAC через `Start-Process -Verb RunAs`; отмена UAC = код 1223 → `RequiresPrivileges`. Клиент через `WindowsPipeClient` (раньше на Windows был mock). `wintun.dll` попадает в пакет через `bundle.resources` (на Windows `$RESOURCE` = каталог экзешника), скачивается `./scripts/fetch-wintun.sh`; helper умеет `console`-режим без регистрации службы. 17 тестов + 2 в helper, 6 тестов упаковки. `scripts/portable.sh` собирает архив «распаковал и запустил». **Не проверено:** реальная служба, `sc.exe`, UAC и wintun — нет Windows VM; `cargo check -p splitroute` под MSVC падает на `ring` (нет MSVC-заголовков). Коммиты `be1d918`, `684e6e5`, `276f218`, `cf26d1a`, `3bf04e9` |
| S3-T5 | Проводка установки helper'а в UI | react-ui | done — `helper_install`/`helper_uninstall` вызывают реальный установщик, ошибки показываются с точной причиной: `InstallerError` маппится в коды `INSTALLER_PRIVILEGES`/`INSTALLER_UNSUPPORTED`/`INSTALLER_MISSING_BINARY`/`INSTALLER_FAILED`, `ErrorBox` показывает локализованный заголовок и текст установщика. Статус берётся из `helper_status` (`shared/lib/helperState.ts`: установлен/не установлен/устарел/не отвечает, по `error.code`), обновляется после действия; пароль не читается и не хранится. Настройки: «Установить»/«Переустановить»/«Удалить», busy-состояние, уведомления об успехе. Тесты: 10 frontend (4 причины + успех + статус + состояния helper'а) и Rust `installer_errors_map_to_distinct_codes` |
| S3-T6 | `THIRD_PARTY_NOTICES`, GPL-3.0-обязательства | оркестратор | done — `THIRD_PARTY_NOTICES` в корне: sing-box 1.14.4 (GPL-3.0-or-later, ссылка на исходники закреплённого тега), wintun 0.14.1 (Prebuilt Binaries License, полный текст), Tauri и плагины, полный инвентарь Rust (511 пакетов по семействам лицензий, MPL-2.0 отдельно), frontend, шрифты/иконки; полные тексты GPL-3.0 (в `LICENSE`), MIT, ISC, BSD-3-Clause, Apache-2.0. Оба файла в бандле: macOS — `Contents/Resources` (проверено), DMG показывает лицензию (проверено), `.deb`/`.rpm`/AppImage — `/usr/share/doc/splitroute/`, NSIS — `licenseFile`. `bundle.license=GPL-3.0-only`, `bundle.licenseFile=../LICENSE`. Тесты упаковки: 10 (наличие файлов, ссылки sing-box, лицензии в конфигах) |
| S3-T7 | Smoke на чистых системах + CachyOS-сценарий (S2-T9) | platform-native | **blocked** — macOS-часть выполнена 2026-11-14: пакет `.app`/`.dmg`/portable собирается, `LICENSE`/`THIRD_PARTY_NOTICES` в Resources побайтово совпадают, DMG показывает GPL-3.0, приложение запускается и завершается без остатков, упакованные helper + sing-box проходят `status/start/stop` без root (сокет и конфиг `0600`, mixed-inbound слушает, очистка после `stop`), helper обслуживает одно сообщение на соединение. **Не проверено и не выдаётся за проверенное:** установка LaunchDaemon (blocked подписью), TUN/per-app/DNS/сон/смена сети на упакованных бинарях (нет root и реальной подписки; функционально проверено в S2-T8/S2-T10 на dev-бинарях), Linux/CachyOS (S2-T9, `systemctl`, `pkexec`, `.deb`/`.rpm`/AppImage) и Windows (`sc.exe`, UAC, pipe ACL, TUN, NSIS) — нет целевых машин. Матрица и точные команды — `docs/stage-3-report.md` §9, `NEXT_SESSION.md` |
| S3-T8 | Отчёт этапа 3 и handoff этапа 4 | оркестратор | done — `docs/stage-3-report.md` §9 (G3.4: окружение, автопроверки, артефакты, smoke helper'а, blocked-пункты, итоговая матрица, лицензии, риски); `docs/research.md` §13 с проверенными фактами; `TASKS.md` отражает статусы; `NEXT_SESSION.md` содержит состояние и промпт G4.1 |

## Этап 4 — полировка (in-progress: S4-T1 done; следующий — G4.2/S4-T2)

| ID | Задача | Владелец | Критерий приёмки |
|---|---|---|---|
| S4-T1 | Каталог установленных приложений (без иконок) | react-ui + rust-core | done — `crates/protocol::InstalledApp {name, path, platform}` (serde, round-trip); команда `apps_installed` (`src-tauri/src/catalog/`): macOS — `.app` в `/Applications`, `/System/Applications`, `~/Applications` без рекурсии; Linux — `.desktop` из `/usr/share/applications` и `~/.local/share/applications`, имя из `Name`, путь из `Exec` (первый токен, `env`-обёртка снимается, относительные и отсутствующие бинари пропускаются); Windows — ярлыки меню «Пуск» (ProgramData + APPDATA) и реестр `Uninstall` (`DisplayIcon`), свой разбор Shell Link. Разбор каждого источника — чистая функция, тесты на фикстурах/временных каталогах; битые записи пропускаются с причиной в логе `Debug`; дедупликация по `path`, сортировка по имени. `app_rule_from_installed` создаёт выключенное правило: macOS — `path_regex ^<escaped .app>/`, Windows/Linux — `process_path`. UI: вкладки «Запущенные / Установленные / Все правила» (PRD §7), поиск по имени, пустые состояния и типизированная ошибка, «Добавить правило» → `rules_upsert`, подсказка о выключенном правиле. **Вкладка «Запущенные» — заглушка** с пояснением: список процессов не входил в S4-T1 (вынесен в backlog). Проверено на macOS 15.8.2: реальный скан нашёл 110 приложений. **Не проверено:** Linux/Windows-скан на реальных ОС (Windows-ветка реестра типизирована под `x86_64-pc-windows-msvc` отдельно, `cargo check -p splitroute` под MSVC падает на `ring`). Пометка несуществующих путей у уже сохранённых правил (PRD §6.3) — вне объёма S4-T1. Коммит `f4e4eab` |
| S4-T2 | Иконки приложений в каталоге | platform-native + react-ui | Иконка берётся из `.app`/`.exe`/`.desktop` без тяжёлых зависимостей, кэшируется в памяти, есть fallback; UI не тормозит на большом каталоге |
| S4-T3 | EN-локализация | react-ui | Все строки — через словари `ru`/`en`; переключатель языка в настройках; тест на отсутствие непереведённых ключей |
| S4-T4 | Статистика трафика сессии | rust-core + react-ui | Счётчики ↑/↓ за сессию из ядра, сброс при подключении, отображение на главной; без телеметрии |
| S4-T5 | Kill switch (fail-closed) | platform-native + rust-core | При падении ядра/туннеля выбранные приложения не выходят напрямую; явное предупреждение; механизм на macOS проверен вручную |
| S4-T6 | Автовыбор и автопереключение локации | rust-core + react-ui | Задержки измеряются, локация переключается при деградации выше порога, функцию можно отключить; переподключение не оставляет TUN |
| S4-T7 | Доменные правила провайдера | rust-core | После решения PRD §13.2: `geoip`/`geosite` из шаблона → route-правила, golden-тесты, `sing-box check` |
| S4-T8 | Автообновление приложения | platform-native + оркестратор | Tauri updater, подписанные обновления, канал; обновление проверено на macOS вручную, секретов в конфиге нет |

Незакрытые блокеры этапа 3 переносятся без потери: **S3-T2** (подпись Apple) и
**S3-T7** (Linux/Windows) перепроверяются, когда появятся сертификат и целевые
машины. Новые задачи этапа 4 получают ID `S4-T<n>` и не переиспользуют старые.

## Этап 5 — macOS NetworkExtension (todo, при подтверждении R1)

- Apple Developer Program, entitlement, Swift-расширение.
- Packet Tunnel/App Proxy для per-app без process-матчинга.
- Нотаризация и распространение.

---

## Backlog

- Список запущенных процессов на вкладке «Запущенные» (PRD §6.3, M4): сейчас
  вкладка показывает заглушку. Нужен источник процессов без привилегий
  (macOS — `NSRunningApplication`/`sysctl`, Linux — `/proc`, Windows — Toolhelp)
  и тот же сценарий «создать выключенное правило».
- Пометка сохранённых правил с несуществующим путём как «неактивно» с
  подсказкой (PRD §6.3): каталог пока проверяет только `Exec` при сканировании
  Linux.
- Поддержка base64/vless-подписок и отдельных ключей.
- Clash YAML.
- Импорт по QR.
- Несколько подписок одновременно.
- IPv6-маршрутизация выбранных приложений (после решения PRD).
- Применять `connect_on_start` при запуске: настройка сохраняется, но пока не используется.
- Автообновление подписки по `auto_update_hours` (M1) и индикация следующего обновления.
- Проверка задержки локаций (M2).
