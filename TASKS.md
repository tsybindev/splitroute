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

## Этап 2 — ядро MVP (in-progress; закрыты G2.1–G2.4, остался S2-T8)

| ID | Задача | Владелец | Критерий приёмки |
|---|---|---|---|
| S2-T1 | Tauri 2 скелет, workspace, CI-команды | оркестратор | `pnpm tauri dev` запускается на macOS и CachyOS — **macOS 15.8.2: done**, `pnpm tauri dev` стартует (Vite + окно); CachyOS **не проверено** (нет машины) |
| S2-T2 | `crates/protocol`: типы IPC и общие модели | rust-core | done — Request/Response/Command/ErrorCode, HelperStatus, Profile, AppRule, Settings; 13 тестов, clippy чистый |
| S2-T3 | Парсер подписки + фикстуры + тесты | rust-core | done — `src-tauri/src/subscription/`: VLESS+Reality/TLS+TCP → `Profile`, пропуски с причинами; тесты на валидный/пустой/нет proxy/битый JSON/неподдерживаемый транспорт + фикстура `spike/fixtures/subscription.json` |
| S2-T4 | Генератор sing-box конфига + golden-тесты | rust-core | done — `src-tauri/src/config/`: TUN, DNS по процессу/системный, allowlist, IPv6 on/off, блок IPv6 для выбранных; golden-файлы + интеграционный `sing-box check` |
| S2-T5 | Клиент helper'а (mock + реальный), установка/удаление | platform-native | done — `src-tauri/src/privileged/`: трейт `HelperClient`, Unix-транспорт (JSON-строки, лимит, таймаут, версия), mock, типизированные ошибки; `Installer`-заглушка (установка — этап 3); tests 8+3+1 |
| S2-T6 | Экраны: Главная, Локации, Приложения, Настройки, Логи | react-ui | done — экраны по PRD §7, i18n ru, все состояния и ошибки; обёртки в `src/shared/api`, состояние в Zustand; тесты 18 (Vitest) |
| S2-T7 | Трей, автозапуск, single instance | react-ui | done — меню трея (статус, подключить/отключить, открыть, выход), `tauri-plugin-autostart`, `tauri-plugin-single-instance`, закрытие в трей; трей визуально **не проверен** (headless), автозапуск в системе не регистрировался |
| S2-T8 | Сквозной сценарий: подписка → локация → подключение → allowlist | оркестратор | Ручная матрица PRD на macOS и CachyOS |

---

## Этап 3 — helper'ы и упаковка (todo)

- macOS: LaunchDaemon, установка/удаление, ad-hoc dev-подпись.
- Windows: служба под SYSTEM, named pipe ACL, `wintun.dll`.
- Linux: systemd + polkit, `.deb`/`.rpm`/AppImage.
- Установщики и smoke-тесты на чистых системах.
- `THIRD_PARTY_NOTICES`, проверка GPL-3.0-обязательств.

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
