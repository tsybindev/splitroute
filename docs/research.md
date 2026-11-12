# Техническое исследование

Дата проверки: 2026-11-12. Документ фиксирует факты, на которых построены PRD и AGENTS.md. Всё, что не проверено запуском, помечено как гипотеза и входит в план spike'а (этап 1).

---

## 1. Per-app маршрутизация в ядрах

| Утверждение | Статус | Источник |
|---|---|---|
| sing-box поддерживает `process_name`, `process_path`, `process_path_regex` в route-правилах | Подтверждено документацией | [Route Rule — sing-box](https://sing-box.sagernet.org/configuration/route/rule/) |
| Эти поля поддерживаются на Linux, Windows и macOS | Подтверждено документацией | там же |
| В Apple-клиентах sing-box (SFI/SFM/SFT) process-правила не работают, потому что трафик идёт через NetworkExtension | Подтверждено обсуждением мейнтейнера | [sing-box #1533](https://github.com/SagerNet/sing-box/issues/1533) |
| CLI-ядро sing-box с process-правилами на macOS работает при наличии прав | Подтверждено обсуждением; требует проверки на целевых версиях macOS | [sing-box #1533](https://github.com/SagerNet/sing-box/issues/1533) |
| mihomo также умеет process-правила, но на macOS 27 у не-Apple-подписанных бинарей они не матчатся | Подтверждено issue с воспроизведением | [mihomo #3355](https://github.com/MetaCubeX/mihomo/issues/3355) |
| Process-правила и UDP работают нестабильно (пример: mihomo, Windows, iperf3) | Подтверждено issue | [mihomo #1800](https://github.com/MetaCubeX/mihomo/issues/1800) |

**Вывод.** sing-box — подходящее ядро для MVP: не нужно писать свой packet-стек, per-app задаётся декларативно. Но на macOS способ зависит от версии ОС и подписи бинаря, поэтому нужен spike и план Б в виде NetworkExtension.

---

## 2. macOS: почему process-правила под угрозой

Issue [mihomo #3355](https://github.com/MetaCubeX/mihomo/issues/3355) (macOS 27.0.1, arm64):

- `sysctlbyname("net.inet.tcp.pcblist_n")` возвращает корректный размер таблицы, но буферизованное чтение у не-Apple-подписанных процессов возвращает только 48 байт (два заголовка `xinpgen`) с `rc=0` и без ошибки.
- Apple-подписанные процессы получают полную таблицу.
- Итог: `findProcessName()` перебирает ноль записей, `PROCESS-NAME`/`PROCESS-PATH` не срабатывают ни в system-proxy, ни (предположительно) в TUN-режиме.
- Автор проверял на mihomo v1.19.32 и alpha; воспроизведение — 30/30.

**Следствия для нас:**

1. На текущей dev-машине (macOS 15.8.2) риск не проявляется, но релизная поддержка macOS 26/27 обязательна.
2. Способ проверки в spike: запустить sing-box с правилом `process_name` для тестового процесса, пройти через TUN и посмотреть, применяется ли правило. Проверить на macOS 15.x и на актуальной 26/27.
3. Если не работает — варианты:
   - деградация: локальный mixed-inbound + запуск браузера с `--proxy-server`, CLI через переменные окружения, ручные настройки приложений;
   - NetworkExtension (правильный путь): Packet Tunnel / App Proxy / Transparent Proxy, требует entitlement `com.apple.developer.networking.networkextension` и Apple Developer Program.

---

## 3. macOS: «правильный» per-app

- Apple поддерживает два механизма per-app: Packet Tunnel Provider (весь трафик приложения в VPN) и App Proxy Provider (трафик приложения в прозрачный прокси). Требуется entitlement и подпись.
- NEFilterDataProvider даёт доступ к метаданным потока, включая приложение-источник, и может использоваться как content filter.
- Официальный клиент sing-box для Apple использует NetworkExtension; именно поэтому в нём нет process-правил.
- Источники: [Routing your VPN network traffic](https://developer.apple.com/documentation/networkextension/routing-your-vpn-network-traffic), [Network Extension](https://developer.apple.com/documentation/networkextension), [sing-box for Apple platforms](https://sing-box.sagernet.org/clients/apple/).

**Вывод.** NetworkExtension — этап 5 roadmap, не MVP. Для MVP используем CLI-ядро + root helper с честным предупреждением о риске.

---

## 4. Формат подписки (проверено запросом 2026-11-12)

Запрос с заголовками из `vpn_to_xray.py` (`User-Agent: Happ/4.1.0`, `X-HWID`, `Accept: application/json`) вернул JSON-массив.

Наблюдения:

- 2 профиля: `🇬🇧 Великобритания`, `🇳🇱 Нидерланды [Fragment]`.
- Все outbound'ы с реальным подключением: `vless`, `security: reality`, `network: tcp`.
- Теги outbound'ов в профиле: `proxy`, `direct`, `block`, `fragment-out`.
- Ключи профиля: `remarks`, `dns`, `inbounds`, `routing`, `outbounds`, `burstObservatory`.
- Inbounds в профиле: `socks`, `http` (порты 10808/10809 в шаблоне скрипта).
- В `xray-all.json` скрипт кладёт полный конфиг, подменяя `outbounds[0]` на `proxy`; профили без тега `proxy` пропускаются.

**Вывод для парсера.** MVP обязан поддерживать `VLESS + Reality + TCP`. Наличие `fragment-out` означает, что в подписке могут быть дополнительные outbound'ы, которые не нужно тащить в наше ядро. Парсер не должен падать на незнакомых протоколах — только помечать профиль как неподдерживаемый.

---

## 5. Стек и экосистема

| Факт | Источник |
|---|---|
| Tauri 2 требует WebKitGTK 4.1 и набор dev-пакетов на Linux; точные списки зависят от дистрибутива | [Tauri: Prerequisites](https://v2.tauri.app/start/prerequisites/) |
| Clash Verge Rev — Tauri 2 + Rust + sidecar mihomo, GPL-3.0; хороший архитектурный референс | [GitHub](https://github.com/clash-verge-rev/clash-verge-rev) |
| FlClash — Flutter, GPL-3.0; референс UX выбора приложений | [GitHub](https://github.com/chen08209/FlClash) |
| sing-box распространяется под GPL-3.0 | [GitHub](https://github.com/SagerNet/sing-box) |
| Happ — проприетарный клиент; используем только UX-идеи | — |

**Вывод.** Tauri 2 + React + Rust + sing-box sidecar — популярный, лёгкий и проверенный на практике стек (Clash Verge Rev). Лицензия проекта должна быть совместима с GPL-3.0.

---

## 6. Окружение разработки (проверено 2026-11-12)

| Параметр | Значение |
|---|---|
| macOS | 15.8.2 (24H232), arm64 |
| Xcode Command Line Tools | установлены (`/Applications/Xcode.app/Contents/Developer`) |
| Node.js | v22.23.2 |
| pnpm | установлен |
| Go | установлен (`/opt/homebrew/bin/go`) — для проекта не нужен |
| Rust / Cargo | не установлены — ставим по AGENTS.md |
| Python 3 | установлен (`/opt/homebrew/bin/python3`) — используется только исходным скриптом |

На CachyOS окружение проверяется на этапе 1 по инструкции AGENTS.md; результаты добавить сюда.

---

## 7. Что осталось проверить (spike, этап 1)

1. **Process-правила на macOS 15.x**: TUN + `process_name`/`process_path_regex` для Chrome, Telegram, CLI-утилиты.
2. **Process-правила на macOS 26/27**: воспроизводится ли ограничение #3355 для sing-box; если да — зафиксировать и включить план Б.
3. **UDP/QUIC**: проходит ли QUIC выбранного приложения через прокси, не ломается ли при `process_name`; решение по блокировке QUIC.
4. **DNS**: разделение DNS по процессу (выбранные → remote, остальные → local) на всех ОС; поведение с `systemd-resolved` и Private Relay.
5. **Root helper**: минимальный прототип на macOS (LaunchDaemon) и Linux (systemd), проверка сокета и прав.
6. **Windows**: TUN + `process_name`, `wintun.dll`, поведение службы под SYSTEM.
7. **Конвертация профилей**: VLESS/Reality из Xray-конфига в sing-box outbound, сверка с `sing-box check`.
8. **Размер и потребление**: бандл и RAM после сборки MVP.

Результаты каждого пункта — с датой, версиями ОС/ядра и ссылками на логи.

---

## 8. Результаты этапа 1 (проверено 2026-11-12)

Окружение: macOS 15.8.2 (24H232), arm64; sing-box 1.14.4 (pinned); Python 3.14.7;
Go 1.27.0; без root. Полный отчёт — [docs/spikes/stage-1.md](./spikes/stage-1.md).

| Утверждение | Статус | Как проверено |
|---|---|---|
| `process_name` и `process_path_regex` работают для TCP на macOS 15 | **подтверждено** | `spike/macos/process_rules_test.py`, 5/5 кейсов |
| helper-процессы Chrome матчатся по `process_path_regex` `.app` | **подтверждено** | лог `router: found process path: …/Google Chrome Helper (Aperitif)…` |
| Процесс матчится по себе, а не по родителю (R2) | **подтверждено** | дочерний `socksprobe`/`curl` матчится собственным путём |
| Process-правила работают для UDP | **подтверждено** | `spike/macos/udp_rules_test.py`, 2/2 |
| Разделение DNS по процессу применяется ядром | **подтверждено** | `spike/macos/dns_rules_test.py`, 2/2 (два разных DNS-сервера) |
| TUN поднимается без root | **опровергнуто (ожидаемо)** | `configure tun interface: operation not permitted` |
| macOS 26/27 и R1 | **не проверено** | ОС недоступна |
| CachyOS / Linux | **не проверено** | машины нет, Docker-демон не запущен |
| Windows | **не проверено** | VM отсутствует |
| QUIC поверх TUN | **не проверено** | нужен реальный сервер и отдельный сценарий |
| DNS-утечки через TUN | **подтверждены** на macOS 15 / sing-box 1.14.4 | см. §9 |
| `override_address` у direct-outbound | **удалён в 1.13+** | ошибка декодера sing-box; заменён route-правилом |
| Прототип helper'а (сокет 0600, start/stop/status) | **подтверждено** | `spike/helper/test_helper.py`, 6/6 |

Практические следствия для этапа 2: пути процессов на macOS канонизировать
(`/private/var/...`); для TUN нужен root/helper; непроверенные платформы нельзя
объявлять поддерживаемыми до отдельных прогонов.

---

## 9. Результаты этапа 2 (проверено 2026-11-12)

Окружение: macOS 15.8.2 (24H232), arm64; sing-box 1.14.4; реальная подписка
(URL/HWID переданы через env, в репозиторий не попадали); dev-helper
`spike/helper/helper.py` под `sudo`. Подробный отчёт —
[docs/stage-2-report.md](./stage-2-report.md).

| Утверждение | Статус | Как проверено |
|---|---|---|
| Реальная подписка отдаёт Happ-JSON только при `User-Agent: Happ/4.1.0` | **подтверждено** | с `SplitRoute/0.1.0` — base64-список `vless://`; с `Happ/4.1.0` — JSON, 2 профиля |
| Реальный TUN поднимается через dev-helper под root | **подтверждено** | TUN `172.19.0.1`, маршрут через `utun`, sing-box запущен |
| `process_name` + `process_path` работают для TCP в TUN | **подтверждено** | `curl` → IP прокси; выключение правила → домашний IP |
| Невыбранный процесс идёт напрямую | **подтверждено** | `python3` → домашний IP при активном TUN |
| Остановка helper'а удаляет TUN и маршрут | **подтверждено** | `ifconfig`/`netstat` после `stop`, sing-box завершён |
| Dev-helper под sudo должен отдавать сокет пользователю и разрешать `SUDO_UID` | **подтверждено** | без этого сокет `0600 root` недоступен клиенту; после правки работает |
| `started_at` от dev-helper'а приходит как float | **подтверждено** | `time.time()` → `1762950000.123`; протокол теперь принимает дробные секунды |
| DNS-правила по процессу применяются к TUN-DNS | **опровергнуто** | `dns/tls[remote] is unreferenced`; `dig` выбранного процесса получает домашний IP |
| `hijack-dns` + `sniff` исправляют DNS-разделение | **опровергнуто** | с `{"port":[53],"action":"hijack-dns"}` и `{"action":"sniff"}` remote всё ещё не используется |
| Remote DoT через прокси работает | **подтверждено** | mixed-only конфиг с `dns.final=remote`: запрос через SOCKS5 вернул IP прокси |
| Трей визуально | **не проверено** | нет разрешений System Events/screencapture |

Практическое следствие: на этой связке (macOS 15 / sing-box 1.14.4) per-app DNS
через TUN не подтверждён. Решение (PRD §6.4, 2026-11-12): пользователь выбирает
`proxy` (весь DNS через прокси, по умолчанию, без утечек) или `system` (DNS ОС,
совместимо, с предупреждением об утечке). Старый режим `process` выводится из UI
и мигрирует в `proxy`.

Реализация S2-T10 (2026-11-12): `DnsMode::{Proxy,System}`, значение `process` из
`state.json` читается как `proxy` (serde alias). В режиме `proxy` конфиг:
`dns.servers` = remote DoT через `proxy` + local; правила `localhost`,
`.local`, `home.arpa` → local; `dns.final = remote`; первым `route`-правилом
`{"port":[53],"action":"hijack-dns"}`. В режиме `system`: только local, без
правил и `hijack-dns`. Golden-конфиги проходят `sing-box check` (1.14.4).
Поведение на реальном TUN (`proxy`: DNS выбранного и невыбранного процесса,
локальные имена; `system`: DNS ОС) **не проверено** — в окружении нет реальной
подписки и root.
