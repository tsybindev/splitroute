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
| S2-T10 | DNS-политика: два пользовательских режима `proxy` и `system` | rust-core + react-ui | done — `DnsMode::{Proxy,System}`, старое `process` читается как `proxy` (тесты serde/миграции); `build_dns`/`build_route`: proxy — remote DoT + local, локальные имена (`.local`, `home.arpa`) → local, `final=remote`, первым `hijack-dns`; system — только local, без правил и hijack; golden обновлены, `sing-box check` зелёный; connect логирует режим и для system пишет Warn; UI — только два варианта с подсказками, бейдж DNS. Реальный TUN-тест `proxy`/`system` — **не проверено** (нет подписки и root в окружении) |

---

## Этап 3 — helper'ы и упаковка (todo)

| ID | Задача | Владелец | Критерий приёмки |
|---|---|---|---|
| S3-T1 | Rust-helper: сокет/пайп, команды, проверка uid/SID, `sing-box check` | platform-native | `crates/helper` реализует `status/start/stop`, лимит сообщения, версию протокола; тесты сериализации, прав сокета, чужого uid и невалидного конфига |
| S3-T2 | macOS: LaunchDaemon, установка/удаление, ad-hoc dev-подпись | platform-native | plist в `/Library/LaunchDaemons`, сокет `/var/run/splitroute` `0600`, установка/удаление через UI с подтверждением; ручная проверка на macOS |
| S3-T3 | Linux: systemd + polkit, `.deb`/`.rpm`/AppImage | platform-native | юнит `splitroute-helper.service`, сокет `/run/splitroute`, установка через `pkexec`; сборки ставятся на CachyOS/Ubuntu |
| S3-T4 | Windows: служба под SYSTEM, named pipe ACL, `wintun.dll` | platform-native | служба ставится/удаляется, пайп доступен только SID пользователя, TUN работает; smoke на VM |
| S3-T5 | Проводка установки helper'а в UI | react-ui | `helper_install`/`helper_uninstall` вызывают реальный установщик, статус и ошибки отображаются, пароль не хранится |
| S3-T6 | `THIRD_PARTY_NOTICES`, GPL-3.0-обязательства | оркестратор | `LICENSE` и notices в бандле, ссылка на исходники sing-box, проверка списка зависимостей |
| S3-T7 | Smoke на чистых системах + CachyOS-сценарий (S2-T9) | platform-native | установка, запуск, подключение/отключение и allowlist на macOS, CachyOS и Windows; результаты в отчёте |
| S3-T8 | Отчёт этапа 3 и handoff этапа 4 | оркестратор | `docs/stage-3-report.md`, обновлённые `TASKS.md` и `NEXT_SESSION.md` |

## Этап 4 — полировка (todo)

- Каталог установленных приложений, статистика трафика.
- Kill switch (fail-closed), автопереключение локации.
- Доменные правила провайдера (после решения по открытому вопросу PRD).
- Автообновление, EN-локализация.

## Этап 5 — macOS NetworkExtension (todo, при подтверждении R1)

- Apple Developer Program, entitlement, Swift-расширение.
- Packet Tunnel/App Proxy для per-app без process-матчинга.
- Нотаризация и распространение.

---

## Backlog

- Поддержка base64/vless-подписок и отдельных ключей.
- Clash YAML.
- Импорт по QR.
- Несколько подписок одновременно.
- IPv6-маршрутизация выбранных приложений (после решения PRD).
- Применять `connect_on_start` при запуске: настройка сохраняется, но пока не используется.
- Автообновление подписки по `auto_update_hours` (M1) и индикация следующего обновления.
- Проверка задержки локаций (M2).
