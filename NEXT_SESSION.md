# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние, что проверено,
> что осталось за пользователем и готовый промпт следующей сессии.

## Состояние

- Этапы 0–4 закрыты (в этапе 2 S2-T9 и в этапе 4 S4-T5c blocked: нет
  CachyOS GUI/root и Windows VM; S3-T2 blocked — нет сертификата Apple).
- Этап 6 (несколько подписок) — done (2026-11-19, `f824f82`).
- **Этап 7 (Clash YAML) — done (2026-11-20, коммит `ceca190`,
  документация — отдельный коммит).** Helper и его протокол не менялись.
- Ветка `main`. CI `.github/workflows/build.yml` не менялся; после пуша можно
  проверить `gh run list` — код CI-пути не затрагивался.
- Факты: [docs/research.md](./docs/research.md) §30 (Clash YAML — детект,
  маппинг узлов, serde_yaml, live-проверка), §29 (несколько подписок).

### Что работает

- **Clash YAML (S7-T1…S7-T3).**
  - Четвёртый формат подписки: тело — YAML-отображение с ключом `proxies`.
    Детект в цепочке `parse_subscription_report`:
    JSON (Happ) → vless-текст → **Clash** → base64 → `INVALID_FORMAT`.
    JSON-объект с `proxies` — Clash в JSON-нотации; `{port: 7890, …}` без
    кавычек ключей — Clash в flow-стиле (фолбэк после ошибки serde_json).
  - Конвертируются только узлы `type: vless` + `network: tcp` + Reality
    (`reality-opts.public-key`/`short-id`) или TLS (`servername`,
    `client-fingerprint`, `alpn`-последовательность). Скип с причиной:
    другие `type`, не-TCP `network`, `tls: false`,
    `skip-cert-verify: true`, битый uuid; неизвестные поля игнорируются.
    Модель `Profile` (crates/protocol) не менялась.
  - `serde_yaml = "0.9"` — обоснование в `src-tauri/Cargo.toml` и PRD §13 п. 9.
  - `errors.INVALID_FORMAT` в `ru.ts`/`en.ts` перечисляет все форматы.
  - Порт строго числом, SNI только из `servername`, `raw` узла —
    YAML→JSON в кэш (в UI вырезается). `proxy-providers` не поддерживаются.
- **Всё из этапов 4 и 6** — без изменений: несколько подписок
  (`SubscriptionState`, расписание per-subscription, guard на подписку,
  UI-группы), автообновление, автоподключение, kill switch (протокол 2),
  статистика (Clash API), каталог и иконки, задержки/автовыбор, обновления
  приложения, локализация, helper'ы и установщики.

### Проверено (2026-11-20)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -D warnings` — чисто. `cargo test --workspace` — **413 тестов** ok
  (1 `#[ignore]`): +16 (11 clash.rs, 4 mod.rs — полный документ,
  JSON-нотация, flow-стиль, NoProfiles; 1 fetch end-to-end с Clash YAML).
  `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc` — чисто.
- `pnpm typecheck`, `pnpm lint`, `pnpm test` — чисто, **103 теста** ok;
  `pnpm build` ok.
- **Live (MockHelper + изолированный `HOME` + локальный HTTP с
  Clash-фикстурой, python-оркестратор по шаблону research §29):**
  seeded `state.json` с подпиской (UA `ClashVergeTest/1.0`, интервал 1 ч,
  пустой кэш) → планировщик сделал ровно 1 запрос с ожидаемыми
  `User-Agent`/`X-HWID`, vless-узел разобран в кэш записи со всеми
  Reality-полями и `fetched_at`, vmess-узел скипнут; через 70 с — 0 новых
  запросов. После проверки процессы остановлены, порты 1420/18081 свободны,
  временный каталог удалён.

### Не проверено (причина)

- **Клик-проверки UI** с реальным Clash-URL — не автоматизировались
  (форма «Подписки» не менялась, формат — серверная часть); покрыты
  Vitest-тестами и тестами команд.
- **Реальный Clash-провайдер** — сетевой путь проверен локальным HTTP;
  у реальных тел бывает `proxy-providers` вместо inline `proxies` (не
  поддерживается — см. research §30 «Ограничения»).
- Всё, что оставалось blocked из этапов 2–4 (см. ниже) — без изменений.

### Ручные проверки, которые остаются blocked

Выполнять только при появлении root/сертификата/целевых ОС и с явным
подтверждением пользователя. Привилегированные действия автоматически не
запускать.

- **Kill switch под root** (S4-T5c): macOS pf / Linux nftables / Windows WFP —
  шаги в `docs/stage-3-report.md` §9 и research §20.
- **S3-T2** — LaunchDaemon: нужен Apple Development/Developer ID
  (`security find-identity` — по-прежнему 0).
- **Linux/CachyOS** (S2-T9, S3-T7) — установка/`systemctl`/`pkexec`/TUN: нет
  GUI и пароля sudo.
- **Windows** (S3-T4, S4-T5b) — служба/UAC/pipe ACL/WFP/TUN: нет VM.
- **Обновления приложения** (S4-T8) — после выбора домена манифеста (PRD §13.2).

### Важные факты окружения

- `export PATH="$HOME/.cargo/bin:$PATH"` в каждом shell; для live-проверок —
  `export RUSTUP_HOME="$HOME/.rustup" CARGO_HOME="$HOME/.cargo"` **до**
  переопределения `HOME`, иначе rustup теряет toolchain.
- **Фоновые процессы, запущенные `&` из shell-команды, не обслуживают TCP в
  этой среде** (bind есть, accept зависает): HTTP-сервер для live-проверок
  держите в foreground-процессе вместе с управлением приложением
  (python-оркестратор: сервер-поток + `subprocess.Popen(start_new_session=True)`,
  порт фикстуры — свой, например 18081, не 1420). Оркестратор хранится
  только в `/tmp` и удаляется после проверки — каждый раз писать заново
  по шаблону research §29/§30.
- macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4, sing-box 1.14.4.
  Цели `x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu` установлены.
- Кросс-`cargo check -p splitroute` невозможен: Windows — `ring`, Linux —
  `gio-sys`/`glib-sys`; платформенные ветки проверять изолированными крейтами.
- `sudo -n true` не проходит; привилегированные действия — только с явным
  подтверждением пользователя.
- Debug-сборка ищет сокет в `$TMPDIR/splitroute-helper.sock`; установленный
  helper v1 несовместим с протоколом 2 — переустановить. Dev-режим:
  `SPLITROUTE_MOCK_HELPER=1`.
- **Состояние:** `<config>/splitroute/state.json` (macOS —
  `~/Library/Application Support/splitroute/state.json`). Секреты не
  логировать; HWID — не секрет, URL подписки — секрет и в логи/диагностику
  не попадает. Ручную правку `state.json` делать по формату текущей версии
  (см. `StoredData`): приложение откатывается к значениям по умолчанию при
  неполном файле.
- Провайдер подписки: формат зависит от UA; `SplitRoute/0.1.0` → base64
  `vless://` (~1 с), `Happ/4.1.0` → JSON (иногда ~17 с). Таймаут 45 с не
  уменьшать. Для Clash-формата — провайдерский UA (например clash-verge);
  заголовок `Accept` не менять (изменение меняло бы поведение существующих
  форматов).
- Скриншоты и фикстуры с реальными URL/HWID/адресами в репозиторий не
  коммитить; фикстуры Clash — example.com/TEST-NET/dummy-uuid.
- После проверок: `pgrep -x sing-box`, `pgrep -fl 'target/debug/splitroute'`,
  `lsof -nP -iTCP:1420 -sTCP:LISTEN`, `ls -d /tmp/sr-*`. Если
  `pnpm tauri dev` упал с «Port 1420 is already in use» — убить `node` на
  порту и `target/debug/splitroute`.

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

## Промпт следующей подцели

Из backlog'а остались: **импорт по QR** (компактная задача, вариант по
умолчанию), **IPv6-маршрутизация** (нужно решение PRD). Блокеры S3-T2/S3-T7/
S4-T5c снимаются только внешними ресурсами (сертификат Apple, root, Windows VM).

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React 19 + TS + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Перед командами: export PATH="$HOME/.cargo/bin:$PATH"

ЦЕЛЬ: реализовать импорт подписки по QR-коду (backlog TASKS.md, задача S8-T1…T3
или следующий этап по согласованию) — ЕСЛИ пользователь в этой сессии назвал
другую задачу, выполняй её. После одной подцели остановись: коммит
кода/тестов, отдельный коммит документации, обновление NEXT_SESSION.md.
Следующие задачи не начинай.

СКОП (уточни у пользователя, если неясно):
- MVP: декодирование QR из ИЗОБРАЖЕНИЯ (файл на диске, вставка из буфера
  обмена, drag&drop в окно) → если внутри URL подписки — предзаполнить форму
  добавления подписки; если vless://-ссылка — отдельное решение в PRD
  (одиночный профиль без подписки сейчас невозможен: подписки — URL-записи).
- Работа с КАМЕРОЙ (AVFoundation, разрешения macOS) — только по явной
  просьбе пользователя в сессии; сама по себе она блокер-не-снимаемая для
  CI/автотестов, предпочтительно отдельной подцелью.
- QR, кодирующий base64-тело или конфиг целиком, не поддерживать: в QR
  провайдеры кодируют URL или vless-ссылку.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §3 (зависимости), §10, §12–§14, §17–§18.
- PRD.md §6.1 (подписки), §6.2 (профили), §13 (принятые решения).
- TASKS.md: этапы 6–7 и backlog.
- docs/research.md §30 (Clash YAML — образец оформления новой задачи),
  §29 (несколько подписок, шаблон live-проверки).
- NEXT_SESSION.md — состояние, что проверено, что blocked.

ТЕКУЩЕЕ СОСТОЯНИЕ
- Этап 7 (Clash YAML) done, коммит `ceca190`: четвёртый формат подписки,
  детект в цепочке `src-tauri/src/subscription/mod.rs`, парсер
  `subscription/clash.rs`. 413 Rust- и 103 frontend-теста, live-проверка
  зелёные.
- Этап 6 (несколько подписок) done, `f824f82`: записи SubscriptionState,
  HWID общий, расписание per-subscription, guard загрузки на подписку,
  UI-группы, карта «Подписки» в настройках.
- UI: форма добавления подписки — `SubscriptionsCard` (черновики полей не
  привязаны к стору); команды `subscription_add/update/delete` в
  `src-tauri/src/commands/mod.rs`.

КОНТРАКТ, КОТОРЫЙ НЕЛЬЗЯ ЛОМАТЬ
- Новых привилегий и shell-команд; helper и его протокол не меняются.
- `crates/protocol` — единственный источник общих моделей; URL подписки,
  UUID и адреса серверов не попадают в логи, фикстуры и ответы UI.
- Декодирование QR — новая Rust-зависимость (например `rqrr`/`quircs`):
  обоснование в коммите и PRD §13, минимальный крейт без тяжёлых
  транзитивных зависимостей; буфер обмена — существующие плагины Tauri
  (проверить, что подключено) или tauri-plugin-clipboard-manager с
  обоснованием.
- Ошибка декодирования/отсутствие QR — типизированный код ошибки
  (`errors.<CODE>` в `ru.ts`/`en.ts`, тест парности ключей), не паника.
- Форма добавления подписки не должна затирать черновики полей фоновым
  обновлением (см. заметки о `drafts` в SubscriptionsCard).

ПРОВЕРКИ
- cargo fmt --all --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- pnpm typecheck && pnpm lint && pnpm test && pnpm build
- cargo check -p splitroute-helper --target x86_64-pc-windows-msvc
- Live-проверка по шаблону research §29/§30 (python-оркестратор:
  сервер-поток + subprocess pnpm tauri dev; фоновые `&`-процессы в этой
  среде не обслуживают TCP; оркестратор писать заново в /tmp/sr-*).
  После проверки остановить всё: `sing-box`, `target/debug/splitroute`,
  порты 1420 и порт фикстуры свободны.

ДОКУМЕНТАЦИЯ
- TASKS.md: статусы задач и ссылка на коммит.
- PRD.md: §6.1 (импорт), при изменении поведения продукта — отдельный
  пункт §13.
- docs/research.md: новый раздел с датой и ограничениями.
- NEXT_SESSION.md: состояние и самодостаточный промпт следующей подцели.

ОГРАНИЧЕНИЯ
- Не менять helper-протокол, kill switch, обновления приложения,
  установщики, иконки и статистику сверх задачи.
- Не выполнять привилегированные действия без явного подтверждения.
- Не оставлять dev-серверы и процессы после проверок.

ОТЧЁТ
- TASKS.md: статус со ссылкой на коммит и кратким описанием.
- NEXT_SESSION.md: состояние, что проверено, промпт следующей подцели.
- Коммиты: feat:/test: для кода и тестов, отдельный docs: для документации.
```

---

## Заметки

- **Этап 7 (Clash YAML):**
  - Порядок веток детекта важен: YAML — надмножество JSON, поэтому
    Happ-JSON проверяется раньше; объект с `proxies` — Clash даже если это
    валидный JSON. Не меняйте порядок JSON → vless → Clash → base64 без
    пересмотра тестов (`unknown_body_is_invalid_format`, `not_an_array_is_error`).
  - `clash::try_parse` возвращает `Ok(None)` для «не Clash»-тел — это часть
    контракта цепочки; ошибки формата (не-список `proxies`) — только когда
    ключ `proxies` действительно есть.
  - Reality определяется по наличию `reality-opts.public-key` независимо от
    флага `tls`; `tls: false` без reality-opts — скип узла.
  - `skip-cert-verify: true` — намеренный скип: insecure-TLS в модели и
    генерации конфига нет, импортированный узел бы не подключился.
  - Порт строго `as_u64`, SNI только `servername` — при появлении реальных
    тел с строковым портом/`server-name` расширить маппинг (research §30).
- **Этап 6 (несколько подписок):**
  - Логика записей — `src-tauri/src/store/mod.rs` (`SubscriptionState`,
    `add/update/delete_subscription`, `set_subscription_profiles`,
    `migrate_legacy`). Миграция требует, чтобы legacy-файл содержал
    непустой `settings.subscription.url`; профили и `fetched_at` берутся
    из плоских полей старого формата.
  - Не убирайте немедленную запись миграции на диск: `id` записи
    (`new_subscription_id`) случайный, без записи он меняется при каждом
    запуске.
  - `set_subscription_profiles` проверяет существование выбранного профиля
    **по новому состоянию** (после замены профилей записи): проверка до
    замены давала «залипший» выбор на пропавший профиль.
  - Попытка обновления запоминается в `subscription_fetch_inner`, а не в
    планировщике: иначе ручная неудачная попытка не переносит расписание
    (PRD §6.1). Планировщик только логирует исход.
  - UI-черновики карты «Подписки» живут отдельно от стора
    (`drafts` в `SubscriptionsCard`): не привязывайте поля формы напрямую
    к `SubscriptionView` — фоновое обновление затирало бы правку.
  - Событие `subscription-updated` теперь payload одной подписки;
    `applyFetchResponse` в `appStore` мержит его в группу и пересобирает
    плоский список. Не храните `fetchedAt`/`nextUpdateAt` на верхнем уровне:
    расписание per-subscription.
- **S4-T14 (автообновление):** интервал `0` выключает автообновление
  записи; пустой URL записи не опрашивается (NoUrl) — пустой URL нельзя
  создать через add/update, только правкой state.json.
- **S4-T13 (автоподключение):** не убирайте подписку на `autoconnect-failed`
  и порядок «сначала слушатели, потом `initialize()`» в `App.tsx`.
- **S4-T5b (Windows):** адреса TUN задублированы в
  `src-tauri/src/config/mod.rs` — меняйте вместе.
- **Kill switch:** протокол 2; после обновления переустановите helper.
- **S4-T4 (статистика):** источник — локальный Clash API (JSON); после
  перезапуска окна — «—» до переподключения.
- **S4-T3 (локализация):** новые строки — в `ru.ts` и `en.ts`; тест запрещает
  кириллицу в `en` и проверяет парность ключей.
- **S4-T2 (иконки):** не переносите чтение иконок в `apps_installed` и в
  `apps_running`.
- **Лицензии:** при смене sing-box/wintun/updater/serde_yaml править
  `THIRD_PARTY_NOTICES`; тест `packaging_config` это проверяет.
- **Упаковка:** `externalBin` только в платформенных конфигах; перед
  `tauri build` — `fetch-core.sh` и `build-helper-sidecar.sh`.
- **Секреты** (URL подписки, HWID, UUID, реальные адреса, `secret` Clash API,
  содержимое pf/nft-правил с адресами) в задачи, фикстуры, логи и коммиты
  не попадают.
