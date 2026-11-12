# Отчёт этапа 2 — ядро MVP

Дата: 2026-11-12. Задачи: S2-T1…S2-T8 (`TASKS.md`).

Короткий вывод: **сквозной сценарий «подписка → локация → подключение →
allowlist» проверен на macOS 15.8.2 с реальной подпиской и root.** Реальный TUN
поднимается, выбранный процесс (`curl`) ходит через прокси, невыбранный
(`python3`) — напрямую, выключение правила возвращает процесс на прямой маршрут,
TUN и маршруты очищаются после остановки. Найден и подтверждён **дефект DNS:
разделение DNS по процессу для TUN не работает**, DNS выбранного приложения
уходит в локальный resolver. Remote DoT через прокси при этом работает. В UI и
лог добавлено предупреждение по PRD §6.4; решение по DNS-политике требует
отдельного согласования. Трей визуально не проверен — нет UI-автоматизации.

## 1. Окружение

| Параметр | Значение |
|---|---|
| ОС | macOS 15.8.2 (24H232), arm64 |
| Ядро | sing-box 1.14.4 (pinned, `scripts/core-version.txt`) |
| Node / pnpm | Node 22, pnpm 11.24.0 |
| Rust | stable, `cargo`/`rustc` из rustup |
| Права | root через `sudo python3 spike/helper/helper.py` (dev-helper) |
| Подписка | реальная, передана через env; URL/HWID в отчёте не сохраняются |
| Helper | `spike/helper/helper.py` (dev, TUN под root) |
| UI-автоматизация | недоступна: System Events / screencapture требуют разрешений |

Секреты (URL подписки, HWID, UUID, адреса серверов) в отчёт, коммиты и тесты не
попадали. Временные файлы с реальным JSON и логами удалены после проверки.

## 2. Методика

1. **Ядро и команды.** Тест `end_to_end_subscription_location_rules_connect`
   проходит тот же путь, что Tauri-команды: разбор подписки → `Store` →
   выбор локации → правила → `build_config` → `HelperClient::start/stop`.
2. **HTTP-слой.** Тесты `subscription::fetch` поднимают одноразовый HTTP-сервер
   на `127.0.0.1` и проверяют реальный `ReqwestFetcher`: 401/403/404/500,
   валидную фикстуру, невалидный JSON, пустой массив и «нет профилей».
3. **Реальная подписка.** `ReqwestFetcher` с дефолтным `User-Agent` и реальными
   `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID`; проверено, что провайдер отдаёт
   Happ-JSON только при `User-Agent: Happ/4.1.0`, иначе — base64-список vless.
4. **Реальный TUN.** Временный harness (не коммитился) собирал конфиг через
   `build_config`, запускал sing-box через dev-helper под root и проверял:
   TUN-адрес и маршрут, внешний IP выбранного `curl`, внешний IP невыбранного
   `python3`, выключение правила, удаление TUN/маршрутов, возврат домашнего IP.
5. **UI.** Vitest + Testing Library: пустые состояния, тексты ошибок и кнопка
   «Повторить», выбор локации, правила `process_name`/`path_regex`, время сессии,
   предупреждение DNS.
6. **Single instance.** Повторный запуск debug-бинаря завершается за 0.2 с,
   первый процесс остаётся один.

## 3. Сквозной сценарий

| Шаг | Результат | Чем подтверждено |
|---|---|---|
| Пустое состояние: нет подписки, локации и правил | ✅ | `commands::tests::end_to_end…`, `ProfilesScreen.test`, `ConnectScreen.test` |
| Ошибка 401 | ✅ | `http_statuses_are_mapped_end_to_end`; текст и «Повторить» |
| Ошибка 403 | ✅ | то же |
| Ошибка 404 | ✅ | то же |
| Невалидный JSON | ✅ | `http_errors_from_body_are_typed`, `invalid_json_is_typed` |
| Пустой список | ✅ | `http_errors_from_body_are_typed` → `EMPTY` |
| Нет поддерживаемых профилей | ✅ | `http_errors_from_body_are_typed` → `NO_PROFILES` |
| Повтор после ошибки | ✅ | `subscription_error_then_retry_succeeds`, кнопка «Повторить» |
| Реальная подписка: 2 профиля VLESS+Reality/TCP | ✅ | `ReqwestFetcher` + реальный URL; 0 пропусков |
| Первый профиль выбран автоматически | ✅ | `Store::set_profiles`, реальный прогон |
| Выбор второй локации | ✅ | `end_to_end…`, `ProfilesScreen.test` |
| Сохранение выбора после перезапуска | ✅ | `end_to_end…` (новый `Store::load`) |
| Правило `process_name` | ✅ | `end_to_end…` + `AppsScreen.test` |
| Правило `path_regex` | ✅ | `end_to_end…` + `AppsScreen.test` |
| Выключение правила | ✅ | `end_to_end…`; реально: `curl` вернулся на прямой IP |
| Подключение через реальный helper | ✅ | TUN `172.19.0.1`, маршрут через `utun`, sing-box запущен |
| Выбранное приложение идёт через прокси | ✅ | `curl` → внешний IP отличается от домашнего |
| Невыбранное приложение идёт напрямую | ✅ | `python3` → внешний IP совпал с домашним |
| Статус и время сессии на Главной | ✅ | реальный `started_at` (float) + `ConnectScreen.test` |
| Отключение и очистка | ✅ | TUN и маршрут удалены, `sing-box run` завершён |
| DNS без утечек | ❌ **дефект; решение — S2-T10** | DNS выбранного `dig` = домашний IP; `dns/tls[remote]` не используется. PRD §6.4: режимы `proxy`/`system` |
| Single instance | ✅ (headless) | второй процесс завершился за 0.2 с, остался один |
| Закрытие окна в трей | ❌ не проверено | нет доступа к UI-автоматизации |
| Tray: иконка, меню, клики | ❌ не проверено | headless, нет разрешений System Events |
| Регистрация автозапуска | ❌ не проверено | специально не включалась |
| CachyOS / Windows | ❌ не проверено | нет машин |

## 4. Матрица PRD (ручная, macOS)

| Проверка | Статус на 2026-11-12 |
|---|---|
| Подключение/отключение | ✅ реальный TUN через dev-helper |
| Выбранное приложение идёт через прокси | ✅ `curl` → IP прокси |
| Невыбранное приложение идёт напрямую | ✅ `python3` → домашний IP |
| Браузер (Chrome/Safari/Firefox) | ❌ не проверялся (нет UI-автоматизации) |
| Telegram | ❌ не проверялся |
| CLI-утилита из терминала | ✅ `curl` по правилу `process_name` + `process_path` |
| DNS без утечек (режим `proxy`) | ✅ DNS выбранного и невыбранного процесса → IP прокси, `localhost` резолвится (см. §5) |
| DNS ОС (режим `system`) | ✅ DNS выбранного процесса → домашний IP (ожидаемая утечка, UI предупреждает) |
| Сон/пробуждение, смена сети | ❌ не проверялось |
| Выход без зависших TUN/процессов | ✅ TUN, маршрут и sing-box очищены |
| Ошибка подписки и восстановление | ✅ ядро + UI-тексты + «Повторить» |

## 5. Найденные дефекты и исправления

1. **`started_at` как дробное число.** Dev-helper на Python отдаёт
   `time.time()` (`1762950000.123`), а `HelperStatus.started_at` был `Option<u64>`.
   `serde_json` отвергал float, и реальное подключение падало бы с
   `HELPER_INVALID_RESPONSE`. Исправлено десериализатором для целых и дробных
   секунд; тест `helper_status_accepts_fractional_started_at`; проверено на
   реальном helper'е.
2. **Дефолтный `User-Agent` не подходил провайдеру.** С `SplitRoute/0.1.0`
   провайдер отдаёт base64-список `vless://`, парсер возвращает `INVALID_JSON`.
   С `Happ/4.1.0` — валидный Happ-JSON, 2 профиля. Дефолт изменён на
   `Happ/4.1.0`; тест в `store`, реальная проверка через `ReqwestFetcher`.
3. **Dev-helper не работал под sudo.** Сокет создавался root'ом с правами `0600`,
   клиент пользователя не мог подключиться; проверка uid сравнивала с `0`.
   Добавлены `SUDO_UID`/`SUDO_GID`: сокет передаётся пользователю, разрешён его
   uid; 3 новых теста, всего 9.
4. **Mock helper: фиксированное время старта.** На Главной время сессии
   показывало бы почти год. Mock берёт текущее время; тест
   `start_records_config_and_current_time`.
5. **Не было способа проверить подключение без root.** Добавлен dev-переключатель
   `SPLITROUTE_MOCK_HELPER=1` (только debug): helper заменяется на
   `MockHelperClient`, в лог пишется предупреждение.
6. **DNS-утечка (S2-T10, реализовано).** На macOS 15.8.2 / sing-box 1.14.4
   DNS-правила с `process_name`/`process_path` не применяются к DNS-пакетам,
   перехваченным из TUN: `dns/tls[remote]` остаётся `unreferenced`, выбранный
   `dig` получает домашний IP. Remote DoT через прокси проверен отдельно на
   mixed-inbound и работает. Решение (PRD §6.4): два пользовательских режима.
   - `DnsMode::{Proxy,System}`; старое `process` из `state.json` читается как
     `proxy` (serde alias), дефолт — `proxy`.
   - `proxy`: remote DoT через прокси + local, локальные имена (`.local`,
     `home.arpa`) → local, `dns.final=remote`, первым `route`-правилом
     `hijack-dns`; process-правил DNS нет.
   - `system`: только local, без правил и `hijack-dns`.
   - UI: только два варианта с подсказками; на Главной бейдж DNS (proxy —
     success, system — warn), для `system` предупреждение об утечке; `connect`
     логирует режим и пишет `Warn` для `system`.
   - Golden-конфиги обновлены и проходят `sing-box check` (1.14.4).
   - Реальный TUN (реальная подписка + dev-helper под root) подтвердил:
     `proxy` — `dig CH TXT whoami.cloudflare` вернул IP прокси, `remote` в логе
     referenced, `dig localhost` → `127.0.0.1`, выбранный `curl` → IP прокси,
     невыбранный `python3` → домашний IP; `system` — тот же `dig` вернул
     домашний IP, TCP выбранного `curl` по-прежнему через прокси; `stop` удалил
     TUN/маршрут и завершил `sing-box`.

## 6. Проверки

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 -m unittest spike/helper/test_helper.py -v
```

Результат на 2026-11-12:

- frontend: 24 теста (6 файлов), typecheck/lint/build — чисто;
- `splitroute`: 64 теста lib + 4 golden + 1 integration;
- `splitroute-protocol`: 14 тестов;
- `spike/helper`: 9 тестов.

`pnpm tauri dev` с `SPLITROUTE_MOCK_HELPER=1` стартует; single instance
проверен. После реального теста ядро остановлено через helper, TUN и маршруты
удалены, `sing-box run` не осталось, `state.json` восстановлен из бэкапа.

## 7. Что осталось и почему

- **DNS-политика** — PRD §6.4 обновлён: два режима, `proxy` (по умолчанию) и
  `system`, старый `process` выводится. Реализация — S2-T10.
- **Трей и закрытие окна** — нужен доступ к UI-автоматизации или ручная проверка.
- **Chrome/Telegram, сон/пробуждение, смена сети** — ручная проверка.
- **Установка helper'а, LaunchDaemon, подпись** — этап 3.
- **CachyOS и Windows** — нужны машины; проверка в этапе 3.
- **Автообновление подписки (M1), проверка задержки (M2), `connect_on_start`** —
  сохраняются в настройках, но ещё не применяются; вынесены в backlog.
