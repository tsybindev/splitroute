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
| Режим `proxy`: DNS выбранного и невыбранного процесса идёт через прокси | **подтверждено (S2-T10)** | реальный TUN: `dig CH TXT whoami.cloudflare` = IP прокси, `remote` referenced в логе |
| Режим `proxy`: локальные имена | **подтверждено** | `dig localhost` → `127.0.0.1` |
| Режим `system`: DNS выбранного процесса уходит локально | **подтверждено** | `dig CH TXT whoami.cloudflare` = домашний IP, без `hijack-dns` |
| Остановка ядра очищает TUN и маршрут | **подтверждено** | после `stop` нет `sing-box run`, адреса `172.19.0.1` и портов |
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
Поведение на реальном TUN подтверждено (2026-11-12, реальная подписка + dev-helper
под root):
- `proxy`: DNS выбранного и невыбранного процесса отдаётся через прокси —
  `dig CH TXT whoami.cloudflare` вернул IP прокси, `remote` в логе не
  `unreferenced`; `dig localhost` → `127.0.0.1`; выбранный `curl` → IP прокси,
  невыбранный `python3` → домашний IP.
- `system`: DNS выбранного процесса уходит локально —
  `dig CH TXT whoami.cloudflare` вернул домашний IP; TCP выбранного `curl`
  по-прежнему идёт через прокси. Это ожидаемая утечка DNS, ради которой в UI
  показывается предупреждение.
- `stop` удаляет TUN/маршрут и завершает `sing-box`.

## 10. Результаты этапа 3, подцель G3.1 (проверено 2026-11-12)

Окружение: macOS 15.8.2 arm64, Rust 1.99.1, sing-box 1.14.4, обычный
пользователь (uid 501), без root и TUN. Подробности и чек-лист —
[docs/stage-3-report.md](./stage-3-report.md).

| Утверждение | Статус | Как проверено |
|---|---|---|
| `UnixStream::peer_cred` в Rust 1.99.1 ещё нестабилен | **подтверждено** | `cargo check` → E0658 `peer_credentials_unix_socket`; используем `getpeereid` (macOS) и `SO_PEERCRED` (Linux) через libc |
| macOS/BSD наследует `O_NONBLOCK` от слушающего сокета | **подтверждено** | принятый сокет сразу отдавал `WouldBlock`, и сообщение >1 МиБ возвращало `BAD_REQUEST`; после `set_nonblocking(false)` на соединении — `TOO_LARGE` |
| Runtime-каталог под root должен быть `0711`, а не `0700` | **подтверждено логикой прав** | при `0700` пользователь не может дойти до `0600`-сокета, даже если сокет ему принадлежит; `0711` даёт обход без листинга, конфиг/лог остаются `0600` root |
| Rust-helper выполняет `sing-box check` и запускает реальный sing-box 1.14.4 | **подтверждено** | mixed-inbound слушает `127.0.0.1`, `status` отдаёт pid и версию, `stop` завершает процесс |
| Сокет и конфиг helper'а создаются с правами `0600` | **подтверждено** | `stat` на реальном запуске (dev, без root) |
| `stop` удаляет конфиг и завершает ядро | **подтверждено** | `kill(pid, 0)` → ESRCH, `config.json` отсутствует |
| Второй экземпляр helper'а не запускается и не удаляет конфиг работающего | **подтверждено** | интеграционный тест: второй процесс завершается с ошибкой, `config.json` первого остаётся |
| Реальный клиент приложения совместим с новым helper'ом | **подтверждено** | `src-tauri/tests/helper_roundtrip.rs`: `UnixHelperClient::status/start/stop` по реальному сокету |
| Ad-hoc/linker-подпись helper'а не запускается как LaunchDaemon на macOS 15.8.2 | **подтверждено** | реальная установка: файлы скопированы, launchd запустил helper, ядро сообщило `AMFI: ... has no CMS blob?` и `Unrecoverable CT signature issue, bailing out`; сервис удалён, сокета нет |
| На машине есть codesigning identity для helper'а | **опровергнуто** | `security find-identity -v -p codesigning` → `0 valid identities found` |
| Установка LaunchDaemon, `launchctl print`, права `/var/run/splitroute` | **blocked** | нужен сертификат Apple Development/Developer ID; ad-hoc не подходит. После проверки файлы и runtime удалены, сервис не загружен |
| Удаление установленных файлов при незагруженном сервисе | **подтверждено** | Uninstall из UI удалил plist, helper, sing-box и `/var/run/splitroute`; процессов не осталось |

Практическое следствие: S3-T2 закрыт по коду, но ручная проверка LaunchDaemon
**blocked** без сертификата Apple Development/Developer ID. Установщик теперь
проверяет подпись до копирования и не оставляет полусобранный сервис; чек-лист
и ожидаемые права — `docs/stage-3-report.md` §6. Debug-сборка приложения
использует `$TMPDIR/splitroute-helper.sock`; для установленного LaunchDaemon
нужен `SPLITROUTE_HELPER_SOCKET=/var/run/splitroute/helper.sock`.

## 11. Результаты этапа 3, подцель G3.2 (проверено 2026-11-12)

Окружение: macOS 15.8.2 arm64, Rust 1.99.1, установлены цели
`x86_64-pc-windows-msvc` и `x86_64-unknown-linux-gnu`. Linux-машины и Windows VM
нет — всё, что требует целевой ОС, помечено **blocked**. Подробности и матрица —
[docs/stage-3-report.md](./stage-3-report.md) §5.1.

| Утверждение | Статус | Как проверено |
|---|---|---|
| `tauri-build` ищет `externalBin` уже на этапе `cargo check` и падает, если файла нет (`resource path ... doesn't exist`) | **подтверждено** | добавили `externalBin` в базовый конфиг → `cargo check` упал; убрали → собралось. Поэтому `externalBin` только в платформенных конфигах |
| `externalBin` в `tauri.linux.conf.json` не влияет на сборку хоста (macOS) | **подтверждено** | `cargo check -p splitroute` на macOS с этим файлом прошёл |
| Tauri ждёт sidecar с суффиксом target triple (`<name>-<triple>[.exe]`) | **подтверждено** | `tauri-utils::resources::external_binaries` строит имя по тройке; без суффикса сборка не находит файл |
| Собранный macOS-бандл содержит sidecar'ы рядом с основным бинарём | **подтверждено** | `SplitRoute.app/Contents/MacOS/`: `splitroute`, `sing-box`, `splitroute-helper` — все три на месте |
| Реальная сборка `.app` и `.dmg` на этой машине проходит | **подтверждено** | 96.7 МиБ и 32.8 МиБ; `pnpm tauri build --bundles app,dmg --config <overlay>` |
| `cargo check -p splitroute --target x86_64-pc-windows-msvc` возможен на macOS | **опровергнуто** | `ring` падает: `fatal error: 'assert.h' file not found` — нет MSVC-заголовков |
| `cargo check -p splitroute --target x86_64-unknown-linux-gnu` возможен на macOS | **опровергнуто** | `gio-sys`/`glib-sys`/`gdk-sys`: `pkg-config has not been configured to support cross-compilation` |
| `cargo check`/`clippy` чистых Rust-крейтов под обе тройки работает | **подтверждено** | `splitroute-helper` целиком под windows и linux; `windows.rs`/`linux.rs`/`client.rs` — изолированно, `-D warnings` |
| `windows-sys` 0.61 даёт нужные API для pipe/службы/SID | **подтверждено** | `ConnectNamedPipe` требует `Win32_System_IO`; `ConvertSidToStringSidW`/SDDL — `Win32_Security_Authorization`; `OpenProcessToken`/`GetCurrentProcess` — `Win32_System_Threading`; `GetTokenInformation`/`TokenUser` — `Win32_Security` |
| `SERVICE_STATUS_HANDLE` (`*mut c_void`) не `Send`/`Sync` | **подтверждено** | компилятор требует `unsafe impl Send/Sync` для хранения в `static`; обёрнут в `StatusHandle` |
| Буфер `Vec<u8>` под `TOKEN_USER` не гарантирует выравнивание | **подтверждено логикой типов** | заменён на `Vec<u64>`; иначе чтение `TOKEN_USER` из `Vec<u8>` — UB |
| Блокирующий `ConnectNamedPipe` нельзя прервать из другого потока | **принято за факт** | режим: ожидание клиента в отдельном потоке, главный ждёт с таймаутом и проверяет флаг STOP; иначе `SERVICE_CONTROL_STOP` подвисал бы |
| `sc.exe create ... binPath= "path with spaces" service --pipe ...` регистрирует службу | **опровергнуто рассуждением** | `sc.exe` разбирает argv: значение кончается на закрывающей кавычке, `service` — «неизвестный параметр». Нужен один экранированный токен (`cf26d1a`) |
| Установка службы, `pkexec`, UAC, `systemctl`, `sc.exe` | **blocked** | нет Linux-машины и Windows VM; выполнять автоматически запрещено |
| Права `/run/splitroute`/pipe и TUN на целевых ОС | **blocked** | нужны реальные системы; на macOS TUN проверен в S2-T8 |

## 12. Результаты этапа 3, подцель G3.3 (проверено 2026-11-12)

Окружение: macOS 15.8.2 arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4,
tauri-bundler из состава CLI. Проверялось на реальной сборке пакета и исходниках
Tauri (`crates/tauri-bundler`, `crates/tauri-utils`).

| Утверждение | Статус | Как проверено |
|---|---|---|
| `bundle.resources` (map) копирует файлы в ресурсы приложения и на этапе `cargo check` | **подтверждено** | `tauri-build` `copy_resources`; после `cargo check` в `target/debug/` появились `LICENSE` и `THIRD_PARTY_NOTICES` |
| macOS `.app` содержит `LICENSE` и `THIRD_PARTY_NOTICES` в `Contents/Resources/` | **подтверждено** | `./scripts/package.sh` → `SplitRoute.app/Contents/Resources/{LICENSE,THIRD_PARTY_NOTICES}` |
| `bundle.licenseFile` встраивает текст лицензии в `.dmg` и показывает его при открытии | **подтверждено** | собранный `.dmg`: при `hdiutil attach` выводится текст GNU GPL v3 и требуется принять лицензию; внутри — те же два файла |
| DMG требует `licenseFile` в корне конфига, а не отдельного ключа `bundle.macos` | **подтверждено** | `macos/dmg/mod.rs`: `settings.license_file()` → аргумент `bundle_dmg.sh` |
| NSIS использует `licenseFile` как страницу лицензии | **подтверждено кодом** | `windows/nsis/mod.rs`: файл копируется в `license_file` и передаётся в шаблон |
| `.deb`/`.rpm`/AppImage не кладут `licenseFile` в `/usr/share/doc` автоматически | **подтверждено кодом** | `debian.rs` обрабатывает только `changelog`; `rpm.rs` берёт `license` только как тег метаданных. Тексты кладутся через `files` |
| `bundle.linux.deb.files` и `bundle.linux.appimage.files` — карта `путь_в_пакете -> источник` | **подтверждено кодом** | `fs_utils::copy_custom_files`: ключ — путь в `data_dir` (deb/appimage); `rpm.rs` использует ключ как абсолютный путь в пакете |
| `bundle.linux.rpm.files` принимает абсолютные пути в пакете | **подтверждено кодом** | `rpm.rs`: `FileOptions::new(rpm_path)` для каждого ключа карты |
| `bundle.license` задаёт SPDX-лицензию в метаданных `.rpm` и `control` | **подтверждено кодом** | `rpm.rs`: `PackageBuilder::new(..., &license, ...)` |
| Фронтенд-поставка ограничена `react`, `react-dom`, `scheduler`, `zustand`, `lucide-react`, `@tauri-apps/api`, `csstype` | **подтверждено** | `pnpm licenses list --prod`: MIT (React/zustand), ISC (lucide-react), Apache-2.0 OR MIT (@tauri-apps/api) |
| В Rust-дереве нет GPL/LGPL/AGPL-зависимостей, кроме собственных крейтов | **подтверждено** | `cargo metadata --all-features`: 511 пакетов; GPL — только `splitroute*`; LGPL — лишь как альтернатива в `r-efi` (выбран MIT/Apache) |
| MPL-2.0 присутствует транзитивно (`cssparser`, `selectors`, `dtoa-short`, `option-ext`) | **подтверждено** | `cargo metadata`; файловый копилефт, исходники не изменялись |
| wintun.dll распространяется по отдельной лицензии WireGuard LLC, требующей сохранять уведомления и не модифицировать DLL | **подтверждено** | `wintun-0.14.1.zip` → `LICENSE.txt`, sha256 совпал с `scripts/fetch-wintun.sh` |
| sing-box 1.14.4 — GPL-3.0-or-later, исходники доступны по тегу | **подтверждено** | `LICENSE` из тега `v1.14.4`: «either version 3 ... or any later version» + оговорка об имени |

## 13. Результаты этапа 3, подцель G3.4 (проверено 2026-11-14)

Окружение: macOS 15.8.2 (24H232) arm64, Rust 1.99.1, Tauri 2.12.5 / CLI 2.12.4,
sing-box 1.14.4. Обычный пользователь (uid 501), без root, без подписи, без
Linux-машины и Windows VM. Подробности и матрица —
[docs/stage-3-report.md](./stage-3-report.md) §9.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Пакет `.app`/`.dmg` собирается из текущего дерева и содержит sidecar'ы | **подтверждено** | `./scripts/package.sh`: 96.80 МиБ / 32.87 МиБ; `Contents/MacOS/{splitroute,sing-box,splitroute-helper}` |
| `LICENSE` и `THIRD_PARTY_NOTICES` в `Contents/Resources` побайтово равны файлам в корне | **подтверждено** | `cmp` на собранном `.app` |
| DMG показывает текст GNU GPL v3 при открытии | **подтверждено** | `hdiutil attach -nobrowse -readonly` вывел текст лицензии |
| Portable-архив содержит приложение, ядро, helper, лицензию и notices | **подтверждено** | `tar -tzf` + распаковка: `sing-box version` → 1.14.4, `splitroute-helper --help` |
| Helper в собранном бандле имеет ad-hoc/linker-подпись | **подтверждено** | `codesign -dv`: `Signature=adhoc`, `flags=0x20002(adhoc,linker-signed)`; приложение — тоже ad-hoc |
| Установщик отклонит helper из бандла до копирования | **следует из проверки подписи** | код проверяет `codesign` до привилегированного шага; identity на машине — 0 |
| Упакованные helper и sing-box проходят `status/start/stop` без root | **подтверждено** | smoke с бинарями из `.app`: сокет `0600`, конфиг `0600`, mixed-inbound слушает, `stop` удаляет конфиг и процесс, `SIGTERM` удаляет сокет |
| Helper обслуживает одно сообщение на соединение | **подтверждено** | второй запрос в тот же Unix-сокет даёт `BrokenPipe`; клиент приложения (`UnixHelperClient::status/start/stop`) открывает новое соединение на каждую команду |
| Приложение из `.app` запускается и завершается без остатков | **подтверждено** | `open -n` → процесс жив; `SIGTERM` → процесса нет, `splitroute-helper`/`sing-box run` не остались |
| Установка LaunchDaemon возможна без сертификата | **опровергнуто повторно** | `security find-identity -v -p codesigning` → 0; helper ad-hoc (см. §10) |
| TUN и per-app на упакованных бинарях | **не проверено** | нет root (`sudo -n` просит пароль) и реальной подписки; ранее проверено на dev-бинарях (§9) |
| Linux/Windows smoke, `.deb`/`.rpm`/AppImage, NSIS | **blocked** | нет целевых ОС; не выдаётся за проверенное |

Практическое следствие для этапа 4: код этапа 3 завершён, но приёмка S3-T2 и
S3-T7 остаётся за внешними условиями (сертификат Apple, Linux-машина, Windows
VM). Скрипты smoke на других ОС должны учитывать «одно сообщение на
соединение». Идентификатор `com.splitroute.app` заканчивается на `.app` —
Tauri предупреждает о конфликте с расширением бандла; финальный bundle id
нужно зафиксировать до публичной сборки (PRD §13).

## 14. Результаты этапа 4, подцель G4.1 (проверено 2026-11-14)

Окружение: macOS 15.8.2 (24H232) arm64, Rust 1.99.1, обычный пользователь
(uid 501). Linux-машины и Windows VM нет.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Каталог macOS находит `.app` в трёх каталогах без рекурсии | **подтверждено** | реальный скан `catalog::scan()`: 110 приложений из `/Applications`, `/System/Applications`, `~/Applications`; имена отсортированы, дубликатов нет |
| Подкаталоги без расширения `.app` (например `/System/Applications/Utilities`) не попадают в каталог | **подтверждено** | скан пропустил `Utilities`, `Python 3.12`, `Chrome Apps.localized` — это контейнеры, а не бандлы; рекурсия сознательно не делается (S4-T1) |
| `.app`-бандл с точкой в имени даёт корректный stem и regex | **подтверждено** | тесты `app_from_bundle` и `escape_regex`; `^/Applications/Google Chrome\.app/` |
| Linux `.desktop` разбирается без обращения к системе | **подтверждено** | тесты на строках: `Name`/`Exec`, локализованные `Name[ru]`, кавычки, `env VAR=1`, field-коды, `Hidden`, `Type=Link`; реальный скан на Linux **не проверен** |
| Битые записи не роняют скан | **подтверждено** | тесты: нечитаемый каталог, битый `.lnk`, `.desktop` с относительным или отсутствующим `Exec` — причина в `skipped`, остальные записи на месте |
| Windows-ветка реестра типизируется под MSVC | **подтверждено** | изолированная `cargo check --target x86_64-pc-windows-msvc` копии функций `collect`/`uninstall_roots`/`read_uninstall_key` с `windows-sys 0.61` — ок; полный `cargo check -p splitroute` под MSVC падает на `ring` (`assert.h not found`), как и в §13 |
| Реальный скан Windows/Linux | **не проверено** | нет целевых ОС; ярлыки и реестр проверены только на синтетических фикстурах |
| Пометка несуществующих путей у сохранённых правил (PRD §6.3) | **не реализовано** | вне объёма S4-T1; каталог помечает только отсутствующий `Exec` при сканировании Linux |

## 15. Результаты этапа 4, подцель G4.2 (проверено 2026-11-14)

Окружение: macOS 15.8.2 (24H232) arm64, Rust 1.99.1, Tauri 2.12.5. Обычный
пользователь (uid 501); Linux-машины и Windows VM нет. Коммит кода — `ae1fe6d`.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Современные `.icns` содержат PNG в `ic07`/`ic11`/`ic13`, но не в `icp4`/`icp5`/`icp6` | **подтверждено** | разобраны `app.icns` Chrome и `AppIcon.icns` Calculator/Telegram/Notes: чанки `ic13`, `ic11`, `ic04` (raw), `ic07`; фиксированный `icp6` не подошёл бы. Поэтому выбор — минимальная ширина ≥ 64 из любых PNG-чанков |
| Реальный скан macOS: 110 приложений, 100 иконок | **подтверждено** | `cargo test -p splitroute --lib catalog::icons::tests::real_catalog_icons_are_found -- --ignored`: скан 0.4 мс, холодная загрузка всех иконок 1.35 с (debug), из кэша 0.12 мс; суммарно 659 КиБ, максимум 12 КиБ на иконку (лимит 256 КиБ) |
| Иконка отдаётся отдельной командой и не увеличивает ответ каталога | **подтверждено** | `InstalledApp` не менялся (round-trip-тест прежний); `app_icon` объявлена `#[tauri::command(async)]`, UI вызывает её лениво по `IntersectionObserver` (rootMargin 200 px) |
| Ленивая загрузка и заглушка работают в реальном окне | **подтверждено визуально** | окно `tauri dev` (mock helper) на вкладке «Установленные»: иконки Adguard, Antigravity, AnyDesk, App Store отрисованы 32×32; автоматизировать клики не удалось — System Events отвечает `-1728` (нет Accessibility), поэтому проверка только визуальная по скриншоту |
| 10 из 110 приложений без иконки — предел выбранного источника | **подтверждено** | у 7 wrapper-бандлов нет своего `.icns` (иконка во вложенном `Wrapper/*.app`), у Calendar/Photo Booth/System Settings она только в `Assets.car`; в UI — заглушка с первой буквой |
| Windows-код иконки типизируется под MSVC | **подтверждено** | изолированный `cargo check --all-targets --target x86_64-pc-windows-msvc` с `windows-sys 0.61` (`PrivateExtractIconsW`, `GetIconInfo`, `GetDIBits`); полный `cargo check -p splitroute` под MSVC по-прежнему падает на `ring` |
| Иконки Linux в рантайме | **не проверено** | нет Linux-машины: `.desktop` `Icon=`, hicolor-PNG/SVG и приоритет размеров проверены синтетическими каталогами; SVG отдаётся как `data:image/svg+xml` без растеризации, XPM не поддержан |
| Иконки Windows в рантайме | **не проверено** | нет Windows VM; проверены только компиляция Win32-вызовов и чистые функции перевода BGRA/маски на macOS |
| Новых пакетов в дереве не появилось | **подтверждено** | `image` уже был транзитивно через `tauri/image-png`; добавлен прямой зависимостью с `features=["png"]`, `Cargo.lock` изменился одной строкой (`splitroute` → `image`). `THIRD_PARTY_NOTICES` править не потребовалось |

## 16. Разбор зависания подписки (проверено 2026-11-14)

Окружение и провайдер те же; URL подписки и HWID не выводятся и не попадают в
документ. Первый фикс — `06701bf`; миграция UA из него отменена в `9fd1bdf`
(собственный UA поддержан через base64, см. §17).

| Утверждение | Статус | Как проверено |
|---|---|---|
| В `state.json` был сохранён `User-Agent: SplitRoute/0.1.0` | **подтверждено** | чтение `~/Library/Application Support/splitroute/state.json`; на тот момент дефолт в коде был `Happ/4.1.0` (`c79cfa7`), сохранённое значение перекрывало его. В S4-T10 собственный UA снова стал дефолтом, миграция отменена |
| Провайдер нестабилен: один и тот же UA то молчит, то отвечает | **подтверждено** | `curl -m 30` дал `000`, 30 с, 0 байт; повтор того же запроса — `200` за 1.13 с с base64. `Happ/4.1.0` один раз ответил `200` за 17.15 с (JSON), другой раз не ответил за 30 с. Формат зависит от UA, а задержка — от сервера |
| Загрузка подписки замораживала всё окно | **подтверждено кодом** | `subscription_fetch` была синхронной Tauri-командой, а Tauri 2 выполняет такие команды на главном потоке (`tauri-macros`: `ExecutionContext::Async` + синхронная функция → `sync_threadpool`); блокирующий `reqwest` держал UI до 20 с |
| Таймаут 20 с был слишком близко к реальному ответу | **подтверждено** | успешный ответ занял 17.15 с; второй запрос легко превышал 20 с. Исправлено: общий таймаут 45 с, `connect_timeout` 10 с |
| `async`-команда не ломает тесты; сохранённый UA не перезаписывается | **подтверждено** | 255 Rust-тестов (`saved_user_agent_is_kept_on_load`), `cargo fmt`, `clippy -D warnings` чисто. Миграция UA из первого фикса удалена в `9fd1bdf` |
| Повторная загрузка подписки из UI после фикса | **не проверено** | клик в окне недоступен (System Events `-1728`); проверены curl-запросы, парсер и компиляция `sync_threadpool`-ветки |
| Base64 нужен для собственного UA | **подтверждено** | с `SplitRoute/0.1.0` провайдер отдаёт `text/plain` base64 со списком `vless://`; реализовано в S4-T10 (§17) |

## 17. Base64/vless-подписки и сборка на Linux (проверено 2026-11-14)

Коммит — `9fd1bdf`. URL подписки, UUID и адреса серверов не выводятся.

| Утверждение | Статус | Как проверено |
|---|---|---|
| С `SplitRoute/0.1.0` провайдер отдаёт base64-список `vless://` | **подтверждено** | `curl`: `200`, 844 байта, `text/plain` (время меняется: 1.13 с и 16.83 с в разных запросах); base64 декодируется в 2 ссылки, домен:443, фрагменты-имена, query-ключи `encryption`, `fp`, `pbk`, `security`, `sid`, `sni`, `type` (Reality/TCP) |
| Реальное тело разбирается нашим парсером | **подтверждено** | временный интеграционный тест на `parse_subscription_report`: **2 профиля, 0 пропусков**, оба Reality/TCP; тело и тест удалены, секреты не коммитились |
| Тело может быть standard/url-safe base64 без паддинга и с переносами | **подтверждено** | собственный декодер: векторы RFC 4648, url-safe, `Zg`, переносы; тесты `subscription::base64` |
| Ссылка разбирается в `Profile` без потерь для MVP | **подтверждено** | тесты `subscription::vless`: Reality (`pbk`/`sid`/`sni`/`fp`), TLS + `flow` + `alpn`, IPv6, TCP; ws/security=none/битый UUID — пропуск с причиной |
| Формат определяется автоматически | **подтверждено** | JSON, base64, открытый текст ссылок; неизвестное тело → `INVALID_FORMAT`; e2e через локальный HTTP-сервер |
| Провайдер нестабилен | **подтверждено** | один и тот же запрос: то `200` за 1.13 с, то таймаут 20–30 с без байт (серверная сторона); таймаут клиента 45 с |
| Linux-сборка AppImage/deb/rpm проходит | **подтверждено** | CachyOS 7.3.3, Rust 1.98.0, pnpm 12.3.4, webkit2gtk 2.54.2; `./scripts/package.sh` → `.deb`/`.rpm` **33.47 МиБ**, AppImage **136.99 МиБ** |
| AppImage содержит helper, ядро, лицензии и `.desktop` | **подтверждено** | `--appimage-extract`: `usr/bin/{splitroute,sing-box,splitroute-helper}`, `usr/lib/SplitRoute/{LICENSE,THIRD_PARTY_NOTICES}`, `usr/share/applications/SplitRoute.desktop`, иконка 512×512 |
| В собранный бинарь попал новый код | **подтверждено** | `strings` по `splitroute`: `INVALID_FORMAT` и `SplitRoute/0.1.0` |
| Linux-тесты проходят | **подтверждено** | `cargo test --workspace` на CachyOS: 168 lib + интеграционные, 0 failed, 1 ignored; реальный скан иконок: **19 приложений, 18 иконок**, холодные 0.69 с, кэш 19 µs |
| AppImage соответствует цели PRD ≤ 80 МиБ | **опровергнуто** | 136.99 МиБ: внутри `sing-box` 78 МиБ и `splitroute` 22 МиБ без strip. `.deb`/`.rpm` 33.47 МиБ укладываются; размер AppImage — отдельная оптимизация (strip, состав), не блокер функциональности |
| Запуск AppImage и установка на Linux | **не проверено** | по SSH нет GUI; `sudo -n` требует пароль, поэтому `dpkg`/`systemctl`/`pkexec` не запускались. TUN/per-app на Linux тоже не проверены |
| Windows-иконки в рантайме | **не проверено** | нет Windows VM (см. §15) |

## 18. EN-локализация (проверено 2026-11-14)

Коммит — `613a27e`. Скриншоты и `state.json` с реальным URL/HWID в репозиторий
не попадали.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Словари `ru` и `en` содержат одинаковый набор ключей | **подтверждено** | тест `i18n.test.ts` сравнивает отсортированные `Object.keys`; `en` типизирован как `Record<MessageKey, string>` |
| В `en` нет непереведённых русских строк | **подтверждено** | тест проверяет отсутствие кириллицы и пустых значений во всех ключах `en`; дополнительно EN-тест рендерит все пять экранов и проверяет `document.body.textContent` |
| Язык по умолчанию — `ru`, старый state без поля читается как `ru` | **подтверждено** | Rust-тесты `legacy_preferences_without_language_read_as_ru`, `language_is_saved_and_reloaded`; `AppSettings::default()` содержит `Language::Ru` |
| Неизвестное значение языка не ломает state | **подтверждено** | `Language` десериализуется вручную: `"en"` → `En`, любое другое → `Ru` (тест `unknown_language_falls_back_to_ru`) |
| Переключение применяется без перезапуска окна | **подтверждено** | реальный `pnpm tauri dev` (MockHelper): в «Настройках» выбран English — сразу сменились вкладки, статус, подписи кнопок и подсказки; возврат на «Русский» так же мгновенен |
| Выбор сохраняется через `settings_update` и переживает рестарт | **подтверждено** | после клика `state.json` содержит `"language": "en"`; после остановки и повторного `pnpm tauri dev` приложение открылось на английском. Обратный выбор записал `"language": "ru"` |
| EN-режим не оставляет русского UI-текста | **подтверждено** | пройдены все пять экранов по скриншотам: Home, Locations, Applications, Settings, Logs. Русскими остаются только внешние данные (имена локаций провайдера, имена приложений) и сообщения ядра в логах — это не строки UI |
| Даты и время следуют языку | **подтверждено** | в EN — `en-US` (`11/14/2026, 1:29:48 PM`, `1:40:01 PM`), в RU — `ru-RU` (`toLocaleString`/`toLocaleTimeString` через `locale()`) |
| Меню трея переводится по выбранному языку | **не проверено визуально** | код `tray.rs` берёт `Language` из настроек при создании и в `settings_update`; подписи `Connect/Disconnect/Connected/Disconnected/Open SplitRoute/Quit` покрыты компиляцией и тестами настроек, но само меню трея в этой сессии не открывалось |
| UI-автоматизация доступна | **подтверждено** | в отличие от прошлой сессии System Events не вернул `-1728`: клик по координатам и `key code` работали, элементы webview видны как кнопки (`button Home`, `button Settings`). Это позволило переключить язык реальными кликами |
| Сообщения Rust/helper'а остаются русскими | **подтверждено** | в EN-логах видна строка `Dev-режим: MockHelperClient, реальный туннель не поднимается`; заголовки ошибок локализованы по коду, `message` установщика показывается как деталь (PRD §8) |
| Новых зависимостей нет | **подтверждено** | i18n — собственные объекты и `useSyncExternalStore` из React; `package.json`/`Cargo.lock` не менялись |

## 19. Статистика трафика: Clash API (проверено 2026-11-14)

Подцель G4.4 (S4-T4), коммит `cda798e`. Окружение: macOS 15.8.2 arm64,
закреплённый sing-box 1.14.4.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Закреплённый sing-box 1.14.4 собран с `with_clash_api` | **подтверждено** | `sing-box version` → `Tags: … with_clash_api …` |
| `experimental.clash_api` с `external_controller: 127.0.0.1:<port>` и `secret` проходит `sing-box check` | **подтверждено** | минимальный конфиг без TUN: `sing-box check -c config.json` → код 0 |
| API задан только на loopback | **подтверждено** | в конфиге адрес `127.0.0.1`; наружу процесс не слушает |
| Без `Authorization: Bearer` запрос отклоняется | **подтверждено** | `GET http://127.0.0.1:19090/connections` → `401` |
| С секретом `/connections` отдаёт суммы | **подтверждено** | ответ `{"connections":[],"downloadTotal":0,"memory":…,"uploadTotal":0}` |
| Счётчики растут при реальном трафике | **подтверждено** | через mixed-inbound скачано 200 000 байт: `downloadTotal=200204`, `uploadTotal=89`; соединение закрыто, суммы сохраняются |
| `/traffic` не используется | решение | поток отдаёт дельты по секундам; `/connections` — кумулятивные суммы без состояния и проще при ошибках |
| Порт выделяется динамически | реализовано | `TcpListener::bind(("127.0.0.1", 0))`, порт читается, сокет закрывается перед `start`; редкая гонка видна как ошибка запуска ядра, а не утечка |
| Secret — 32 hex из CSPRNG на сессию | реализовано | `uuid::Uuid::new_v4().simple()`; попадает только в конфиг ядра, не в логи и диагностику |
| Опрос не блокирует окно | реализовано | `#[tauri::command(async)]`, таймаут 2 с, один HTTP-клиент на процесс, лимит тела 1 МиБ |
| Ошибка метрик не влияет на подключение | **подтверждено тестами** | закрытый порт → `available=false`, одно предупреждение в лог; UI показывает «—» |
| Mock-ядро не ломает главную | **подтверждено** | `pnpm tauri dev` с `SPLITROUTE_MOCK_HELPER=1`: «Подключено», время сессии идёт, «Трафик за сессию: —», `ErrorBox` нет |
| Реальные ↑/↓ в окне через helper и TUN | **подтверждено** | release-сборка, helper под `sudo`, sing-box 1.14.4: за 30 с — ↑ 133,8 КиБ / ↓ 1,9 МиБ, обновление раз в секунду без перезагрузки окна |
| После падения ядра трафик «—» и ровно один `Warn` | **подтверждено** | `sudo pkill -x sing-box` при подключённом окне: трафик пропал, в «Логах» одна строка «Статистика трафика недоступна…» |
| Clash API доступен только на loopback и требует secret | **подтверждено** | `sudo lsof -nP -iTCP -sTCP:LISTEN`: `127.0.0.1:<port>`; порт из `/var/run/splitroute/config.json`; `curl` без `Authorization` → `401` (secret не печатался) |
| Linux и Windows в рантайме | **не проверено** | нет машин; Clash API и HTTP-чтение платформонезависимы |

Замечания:

- Считается весь трафик, прошедший через ядро (proxy и direct через TUN), а не
  только proxy.
- Серия живёт в памяти приложения: после перезапуска окна helper может остаться
  запущенным, но порт и secret потеряны, поэтому до переподключения статистика
  показывает «—».
- Порт и secret не сохраняются в `state.json`, не пишутся в логи и не попадают
  в экспортируемую диагностику.
- `Warn` содержит порт сессии, которая не ответила. Если параллельно работали
  два экземпляра (dev и release), у каждого свой helper и своё ядро со своим
  портом — номера в `Warn` и в `lsof` могут не совпадать. Для чистой проверки
  держите один экземпляр приложения и один helper.

## 20. Kill switch (fail-closed): решение и ограничения (проверено частично, 2026-11-14)

Подцель G4.5 (S4-T5a), коммит `2405144`. Окружение: macOS 15.8.2 arm64,
Rust 1.99.1, sing-box 1.14.4; `sudo -n true` не проходит, поэтому применение
firewall в рантайме **не проверено**.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Семантика fail-closed: при падении ядра правила остаются и прямой egress блокируется | реализовано | `HelperCore::reap` не трогает kill switch; правила снимаются только `stop`, `Drop` helper'а, выходом приложения или выключением настройки |
| Очистка при старте не даёт «залипших» правил | реализовано | `cleanup_stale` вызывается после успешного `bind` сокета/валидации pipe; повторный экземпляр helper'а не может снять защиту работающего |
| Включение идемпотентно | **подтверждено тестами** | `KillSwitch::sync(true/false)` повторно не вызывает backend; ошибка enable → `KILL_SWITCH_FAILED`, ошибка disable оставляет `enabled=true` и `error` |
| Старт с включённой защитой не поднимает ядро при ошибке прав | **подтверждено тестами** | fake firewall: `start(..., true)` → `KILL_SWITCH_FAILED`, конфиг не пишется |
| Неудачный старт снимает только правила этого вызова | **подтверждено тестами** | при `was_active=false` — disable один раз; при правилах от прошлого падения — остаются (`enabled=true`) |
| `start` передаёт режим, `status` возвращает состояние | **подтверждено тестами** | протокол 2: `StartPayload.kill_switch`, `HelperStatus.kill_switch`; serde-тесты, mock, команда `connect` |
| `/etc/pf.conf` на macOS 15.8.2 содержит `anchor "com.apple/*"` | **подтверждено** | чтение файла 2026-11-14: `scrub-anchor "com.apple/*"`, `anchor "com.apple/*"`, `load anchor "com.apple" from …` — дочерний anchor применяется без правки системного файла |
| `pfctl` требует root | **подтверждено** | `/sbin/pfctl -s info` → `pfctl: /dev/pf: Permission denied`; поэтому `pfctl -E/-a/-F/-X` в рантайме не выполнялись |
| Реальные правила pf/nft под root | **не проверено** | нет пароля sudo — `S4-T5c` |
| nftables на Linux | **не проверено** | нет Linux-машины с GUI/root; текст правил и классификация ошибок покрыты тестами на macOS |
| Windows: механизм — WFP, а не `netsh advfirewall` | **подтверждено документацией** | брандмауэр Windows отдаёт приоритет блокирующим правилам, поэтому allow-list без смены глобальной политики не выражается; WFP-подслой с весами позволяет разрешить TUN и ядро выше блока (подход WireGuard/Tailscale/Mullvad) |
| Windows: статический план фильтров | **подтверждено тестами** | 6 тестов на macOS: один блок на слой ниже всех разрешений, loopback/TUN/ядро на обоих слоях, те же локальные сети, DHCP, отсутствие данных от клиента, фиксированные TUN-адреса |
| Windows: применение WFP в рантайме | **не проверено** | нет Windows VM: `cargo check`/`clippy -p splitroute-helper --target x86_64-pc-windows-msvc` зелёные, но фильтры не устанавливались; `S4-T5c` |
| Windows: трафик приложений в TUN разрешается по локальному адресу | **не проверено** | условие `FWPM_CONDITION_IP_LOCAL_ADDRESS = адрес TUN` должно совпадать при маршруте в TUN и перестать совпадать после исчезновения адаптера; в рантайме не подтверждено |
| Windows: правила переживают падение helper'а и снимаются при старте | **подтверждено кодом** | сессия WFP не dynamic; `cleanup_stale` удаляет фильтры, подслой и провайдер при старте helper'а |
| `sing-box check` не ломается | **подтверждено** | конфиг ядра не менялся; golden/интеграционные тесты зелёные |

Решение и механизм:

- Правила блокируют весь прямой egress непривилегированных процессов: в pf нет
  фильтра по пути процесса, а per-process firewall на macOS недоступен.
  Разрешены loopback, локальные сети (`10/8`, `172.16/12`, `192.168/16`,
  `169.254/16`, multicast, IPv6 link-local/ULA/multicast) и трафик uid 0 —
  ядро работает под root. Поэтому при мёртвом туннеле прямой выход теряют все
  непривилегированные процессы, а не только выбранные (PRD R14); локальная сеть
  и DNS к локальному resolver остаются доступны. Это осознанно задокументировано
  в UI.
- macOS: anchor `com.apple/splitroute.killswitch`. `pfctl -E` включается один раз,
  токен ссылки хранится в `<runtime>/killswitch.token` (`0600`); при снятии —
  `pfctl -a … -F all` и `pfctl -X <token>`. Ошибки «anchor not found» и
  «token not found» считаются успехом для идемпотентности.
- Linux: отдельная таблица `inet splitroute_killswitch`; перед загрузкой
  удаляется (отсутствие — успех), затем `nft -f <runtime>/killswitch.nft`.
- Windows: собственный провайдер и подслой WFP (`PROVIDER_KEY`/`SUBLAYER_KEY` в
  `killswitch/windows.rs`), вес подслоя `u16::MAX`, сессия **не** dynamic —
  правила переживают падение helper'а. `enable()` в одной транзакции удаляет
  свои объекты, добавляет провайдер, подслой и фильтры; `disable()` тем же
  способом удаляет их. Порядок обязателен: WFP не даёт удалить объект, пока на
  него ссылаются, поэтому сначала перечисляются и удаляются фильтры по
  провайдеру, затем подслой, затем провайдер. «Не найдено» — успех, снятие
  идемпотентно.
- Windows, что разрешено (веса 106…110 против блока с весом 1, слои
  `ALE_AUTH_CONNECT_V4/V6`): loopback (`FWP_CONDITION_FLAG_IS_LOOPBACK`),
  локальные сети и multicast, DHCP `UDP 68 → 67` на `255.255.255.255`, трафик с
  локальным адресом соединения, равным адресу TUN, и трафик процесса ядра по
  `ALE_APP_ID` (`sing-box.exe`). Блокирующий фильтр без условий стоит ниже всех.
  Эквивалент «трафика root» на Windows — точный AppId ядра, а не все службы
  SYSTEM: это уже, чем на Unix, и не требует строить security descriptor.
- Windows: трафик приложений в TUN разрешается по локальному адресу, а не по
  LUID интерфейса, потому что правила ставятся **до** запуска ядра, когда
  wintun-адаптера ещё нет. Адрес TUN фиксирован (`172.19.0.1`,
  `fdfe:dcba:9876::1`) и совпадает с генератором конфига; при падении ядра
  адаптер и адрес исчезают, условие перестаёт совпадать — правила остаются и
  работают fail-closed. LUID-подход WireGuard здесь не подходит без второго
  этапа после старта ядра.
- Windows: `netsh advfirewall` отвергнут. Брандмауэр Windows применяет
  блокирующие правила раньше разрешающих, поэтому «разрешить ядро и TUN,
  заблокировать остальное» без смены глобальной политики outbound не
  выражается; смена политики затронула бы все службы и систему целиком.
- Протокол поднят до версии 2, потому что helper v1 молча проигнорировал бы
  `kill_switch` и клиент считал бы трафик защищённым. Несовместимость
  обнаруживается на `ProtocolMismatch`, UI предлагает переустановку.
- Сон/пробуждение и смена сети: правила не содержат адресов сервера и не зависят
  от интерфейса; DHCP-клиент разрешён отдельным правилом. Отдельного
  пересоздания правил не требуется, но в рантайме это не проверялось.
- Выключение настройки возвращает прежнее fail-open поведение; при следующем
  `start` helper снимет правила.

Ограничения и риски:

- Защита шире allowlist и может восприниматься как «пропал интернет»; снимается
  кнопкой «Отключить и снять защиту» или выключением настройки.
- Если процесс helper'а убит `SIGKILL`, правила остаются до следующего запуска
  helper'а (он очищает их при старте) или до ручного `pfctl`/`nft`. Это
  fail-closed, а не утечка.
- Namespace `com.apple/*` — документированный wildcard в `/etc/pf.conf`; при
  изменении политики Apple механизм нужно перенести на собственный anchor,
  добавляемый установщиком.
- **Windows не проверен в рантайме.** Нет VM: проверены план фильтров,
  `cargo check`/`clippy` под `x86_64-pc-windows-msvc`, но не применение WFP,
  не матчинг TUN-адреса и не поведение при падении ядра — `S4-T5c`.
- **Windows: статические объекты WFP теряются при остановке BFE/перезагрузке.**
  Helper не следит за перезапуском BFE; при следующем `start` правила ставятся
  снова. Пока helper работает, правила на месте.
- **Windows: адрес TUN задублирован** в `WINDOWS_TUN_V4/V6` и в
  `src-tauri/src/config/mod.rs`. Менять их нужно вместе; тест в helper фиксирует
  текущие значения. Общий крейт для продуктовых констант не вводился, чтобы не
  менять `crates/protocol` в этой задаче.
- **Windows: AppId строится из `--sing-box`.** Если путь к ядру неверен или файл
  отсутствует, `enable()` возвращает `KILL_SWITCH_FAILED` и ядро не стартует —
  это безопасный отказ, а не молчаливое разрешение.
- **Windows: входящий трафик не блокируется.** Kill switch закрывает прямой
  egress; входящие соединения остаются на усмотрение брандмауэра Windows.
- Для WFP helper должен работать под SYSTEM/Administrator; консольный dev-режим
  без прав вернёт ошибку WFP, ядро не стартует.

## 21. Задержки локаций и автопереключение (проверено частично, 2026-11-14)

Подцель G4.6 (S4-T6), коммит `2405d09`. Окружение: macOS 15.8.2 arm64,
Rust 1.99.1, sing-box 1.14.4. Реальный провайдер и root в сессии не
использовались, поэтому замеры на живых серверах **не проверены**.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Замер не требует ядра, helper'а и секретов | **подтверждено кодом** | `latency::TcpProbe` делает TCP-connect до `server:port`; UUID, Reality-ключи и конфиг не используются, helper не вызывается |
| TCP-connect измеряется корректно | **подтверждено тестами** | loopback-порт с открытым listener → `Ok(мс)`, закрытый порт → `PING_UNREACHABLE`, неразрешимое имя → `PING_DNS` |
| Ошибки типизированы | **подтверждено** | `PING_TIMEOUT`/`PING_UNREACHABLE`/`PING_DNS`; serde-тест стабильности кода |
| Замер не блокирует окно | реализовано | `#[tauri::command(async)]`; замеры идут в `std::thread::scope` по 8 профилей, лимит 64 профиля |
| Таймаут ограничивает каждое TCP-подключение | **подтверждено кодом** | `TcpStream::connect_timeout` с 3 с; у имени несколько адресов — таймаут применяется к каждому |
| DNS-резолв ограничен тем же таймаутом | **опровергнуто** | `to_socket_addrs` использует системный resolver и отдельного таймаута не имеет; при зависшем resolver общий вызов может быть дольше 3 с. В PRD это оговорено |
| Лучшая локация выбирается детерминированно | **подтверждено тестами** | минимум по `latency_ms`, при равенстве — первый профиль в порядке подписки; ошибки не участвуют |
| Гистерезис и подтверждения работают | **подтверждено тестами** | улучшение меньше порога игнорируется; переключение только после `confirmations` подряд; серия сбрасывается при исчезновении деградации |
| Серия неудач текущей локации ведёт к переключению | **подтверждено тестами** | после `failures_to_switch` неудач и при рабочей альтернативе возвращается кандидат; если альтернатив нет — `None` |
| Выключенная настройка не копит состояние | **подтверждено кодом** | трекер сбрасывается, `switch_to = None` |
| Переключение переподключает туннель | **подтверждено тестами** | `profile_switch`: `stop` → выбор → `start`; mock helper фиксирует `stop_calls=1`, `start_calls=2`, новый сервер в конфиге |
| Без туннеля переключение только сохраняет выбор | **подтверждено тестом** | `switch_without_tunnel_only_saves_selection`: `start_calls=0`, состояние `stopped` |
| Неудачный старт возвращает прежнюю локацию | **подтверждено тестом** | fake helper падает на втором `start`; выбран прежний профиль, ядро снова `running`, kill switch передан заново |
| Kill switch не выключается ради переключения | **подтверждено тестом** | флаг `kill_switch` передаётся в оба `start`; после восстановления `last_kill_switch=true` |
| Адреса серверов не попадают в логи | **подтверждено тестом** | в логах только имя локации и число/код; проверка на отсутствие `203.0.113` |
| Старые `state.json` читаются без миграции | **подтверждено** | `#[serde(default)]` на `auto_select`; дефолты выключены, тест legacy-настроек |
| Диапазоны настроек нормализуются | **подтверждено тестом** | порог ≤ 2000 мс, интервал 15…3600 с, подтверждения и неудачи 1…5 |
| `sing-box check` остаётся зелёным | **подтверждено** | конфиг не менялся; `config_golden` и интеграционные тесты проходят |
| UI показывает задержки, сортирует и помечает лучшую | **подтверждено тестами** | 40 мс выше 180 мс, ошибка показана локализованным текстом, «Лучшая» на минимуме |
| Автопереключение из UI | **подтверждено тестом** | при `auto_switch=true` и подключении вызывается `profiles_ping`, затем `profile_switch` по `switch_to` |
| Окно с MockHelper не ломается | **подтверждено** | `pnpm tauri dev` (`SPLITROUTE_MOCK_HELPER=1`): окно `splitroute` открывается; после проверки процесс и порт 1420 остановлены |
| Реальная задержка серверов провайдера | **не проверено** | нет реального профиля и сети провайдера в сессии; тесты используют закрытые порты и fake-probe |
| Замер на Linux/Windows | **не проверено** | код платформонезависим (`std::net`), но рантайм на этих ОС не запускался |

Решение и ограничения:

- Замер — TCP-connect, а не запрос через прокси: дешево, не требует helper'а и
  работает до подключения. Число не учитывает TLS/Reality и перегрузку сервера
  (PRD R15), в UI подписано как TCP-задержка.
- Автопереключение — opt-in. Решение принимает Rust, UI только опрашивает
  `profiles_ping` по интервалу и выполняет `profile_switch`.
- `stop` на время переключения снимает правила kill switch: между `stop` и
  повторным `start` есть короткое окно fail-open. При ошибке старта приложение
  сразу возвращает прежнюю локацию, но абсолютной атомарности без нового
  протокола helper'а нет; менять протокол ради этого в S4-T6 не стали.
- DNS-резолв не ограничен отдельным таймаутом: при зависшем системном resolver
  проверка может занять больше 3 с. Это ограничение задокументировано, а не
  замаскировано.
- Порог деградации сравнивает TCP-задержки; при равенстве (в пределах порога)
  переключения нет — это защита от «дёрганья» туннеля.
- Счётчики серий живут в памяти приложения и сбрасываются при ручном выборе,
  переключении и `disconnect`.

## 22. Автообновление приложения (проверено частично, 2026-11-14)

Подцель G4.8 (S4-T8). Окружение: macOS 15.8.2 arm64, Rust 1.99.1, Tauri
2.12.5 / CLI 2.12.4, `tauri-plugin-updater` 2.14.1 (features `rustls-tls`,
`zip`). Реальный домен обновлений и публикация артефактов в сессии недоступны,
поэтому установка обновления в рантайме **не проверена**.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Плагин инициализируется, приложение стартует | **подтверждено** | `pnpm tauri dev` (MockHelper): окно `SplitRoute` открывается, процесс работает; после проверки остановлен |
| Placeholder-домен даёт `UPDATE_NOT_CONFIGURED` без сети | **подтверждено тестом** | `updates::endpoint` при `updates.splitroute.invalid` возвращает ошибку; тест не делает запросов |
| Канал нормализуется, старые `state.json` читаются | **подтверждено тестами** | `stable`/`beta`, неизвестное значение → `stable`; отсутствующий блок → проверка выключена, канал `stable` |
| Подписанный артефакт собирается | **подтверждено** | `./scripts/package.sh` с ключом: `SplitRoute.app.tar.gz` (34.5 МиБ) и `SplitRoute.app.tar.gz.sig` |
| Подпись содержит версию | **подтверждено** | trusted comment подписи: `timestamp:… file:SplitRoute.app.tar.gz version:0.1.0`; поэтому включён `requireSignedVersion` |
| Приватный ключ вне репозитория | **подтверждено** | `~/.commandcode/splitroute-updater.key` и пароль `.key.pass` — права `0600`; в git только публичный ключ в `tauri.conf.json` |
| UI: версия, канал, проверка, ошибки, прогресс | **подтверждено тестами** | 7 тестов `UpdateCard`: «не настроено», «обновлений нет», доступная версия, установка, прогресс 50%, ошибка подписи, «Позже» |
| Реальная установка и перезапуск | **не проверено** | нужен HTTPS-хостинг манифеста и подписанный артефакт; локальный self-signed TLS updater не принимает |
| Windows/Linux-артефакты обновления | **не проверено** | нет VM/машины; формат создаёт Tauri bundler, но `.exe`/`.AppImage` не собирались |
| Обновление helper'а вместе с приложением | **не реализовано осознанно** | helper установлен вне бандла (LaunchDaemon/systemd/служба); его обновляют переустановкой из настроек |
| Откат неудачной установки | **не реализовано** | updater заменяет приложение; отдельного rollback нет |
| Реальный манифест `latest.json` | **не проверено** | домен не выбран (PRD §13.2); формат — стандартный static JSON Tauri updater |

Решение и механизм:

- Выбран `tauri-plugin-updater` **только на стороне Rust**: JS-плагин и
  `tauri-plugin-process` не нужны. Команды `update_status`, `update_check`,
  `update_install` возвращают типизированные коды (`UPDATE_NOT_CONFIGURED`,
  `UPDATE_CHECK_FAILED`, `UPDATE_SIGNATURE`, `UPDATE_DOWNLOAD_FAILED`,
  `UPDATE_INSTALL_FAILED`, `UPDATE_NOT_AVAILABLE`, `UPDATE_CONFIG`); прогресс
  загрузки приходит событием `update-progress`, перезапуск — `AppHandle::restart`.
- Endpoint: `https://updates.splitroute.invalid/<channel>/latest.json` —
  заглушка до выбора домена. Только HTTPS; `dangerousInsecureTransportProtocol`
  не включается. В debug-сборке endpoint можно переопределить
  `SPLITROUTE_UPDATE_ENDPOINT` (тоже HTTPS), в release переменная игнорируется.
- Проверка: вручную из настроек; при запуске — только если включён
  `preferences.updates.check_on_start` и канал настроен. Проверка никогда не
  устанавливает обновление.
- Установка: проверка → остановка ядра (TUN и правила kill switch не должны
  пережить замену) → загрузка с проверкой подписи → перезапуск. Если проверка
  не удалась, работающий туннель не трогается.
- Подпись: ключ создан `tauri signer generate`; приватный ключ и пароль лежат в
  `~/.commandcode/` (в git не попадают), публичный — в
  `src-tauri/tauri.conf.json`. `requireSignedVersion: true` не даёт подменить
  версию в манифесте и подсунуть старый подписанный артефакт.
- `scripts/package.sh` требует `TAURI_SIGNING_PRIVATE_KEY` и
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` и добавляет
  `bundle.createUpdaterArtifacts`; для локальной сборки без обновлений —
  `SPLITROUTE_ALLOW_UNSIGNED=1`. Обычный `pnpm tauri build` без ключа работает:
  `createUpdaterArtifacts` не входит в базовый и платформенные конфиги.
- Настройки `preferences.updates { check_on_start, channel }` добавлены с
  `#[serde(default)]`; старые `state.json` читаются с выключенной проверкой и
  каналом `stable`.

Ограничения и риски:

- **Домен и хостинг не выбраны.** До решения PRD §13.2 проверка возвращает
  «канал не настроен», а не сетевую ошибку; публичного манифеста нет.
- **Реальная установка не проверена.** Не проверены замена `.app`, перезапуск,
  поведение macOS Gatekeeper/notarization и Windows NSIS/UAC. Это остаётся
  ручной проверкой после появления домена.
- **Приватный ключ — единственная точка доверия.** Потеря ключа или пароля
  делает невозможной подпись следующих обновлений; компрометация требует новой
  сборки с новым публичным ключом и ручного перехода пользователей.
- **Публичный ключ зашит в сборку.** Ротация ключа возможна только новой
  версией приложения; старые версии не примут обновления, подписанные новым
  ключом.
- **Helper не обновляется автоматически.** После обновления приложения старый
  helper может не совпасть по протоколу; UI покажет `HELPER_PROTOCOL` и
  предложит переустановку. Это осознанно: менять привилегированный сервис во
  время замены приложения рискованно.
- **Нет отката.** Если новая версия окажется хуже, пользователь откатывается
  вручную установкой предыдущего пакета; updater downgrade не выполняет.

## 23. Список запущенных процессов (проверено частично, 2026-11-14)

Подцель G4.9 (S4-T11). Окружение: macOS 15.8.2 arm64, Rust 1.99.1, обычный
пользователь (uid 501). Привилегий, helper'а и внешних команд нет: процессы
читаются напрямую из ОС. Linux- и Windows-ветки компилируются изолированно, в
рантайме на целевых ОС **не проверены**.

| Утверждение | Статус | Как проверено |
|---|---|---|
| macOS: `proc_listpids` + `proc_pidpath` дают pid и абсолютный путь без root | **подтверждено** | реальный скан на машине: 64–68 процессов после фильтра, 2.1–3.2 мс на весь обход; путь каждого процесса непустой |
| `PROC_ALL_PIDS` отсутствует в `libc` 0.2.191 | **подтверждено** | константа объявлена локально (`1`) рядом с вызовом; иначе код не собрался бы |
| Системные процессы можно отбросить по префиксам путей | **подтверждено** | на macOS отброшено 345–347 записей (`/System/Library`, `/usr/libexec`, `/usr/sbin`, `/sbin` и т. п.); `/usr/bin` и `/System/Applications` сохранены как легитимные цели |
| Недоступные процессы не ломают список | **подтверждено** | 1 запись с недоступным путём попала в `skipped`, вкладка получила валидный список |
| Дедупликация по пути, сортировка по имени, лимит 500 | **подтверждено тестами** | несколько копий одного бинаря дают одну строку с первым pid; регистронезависимая сортировка; `truncated` считает скрытое |
| macOS: путь внутри `.app` превращается в `path_regex` по внешнему бандлу | **подтверждено тестами** | `bundle_root` берёт первое вхождение `.app/`, поэтому helper внутри `Google Chrome.app` даёт `^/Applications/Google Chrome\.app/`; правило выключено |
| Внешние процессы без `.app` (например, CLI) получают `process_path` | **подтверждено тестами** | `/usr/bin/curl` → `process_path`, `path_regex` пуст |
| Windows: Toolhelp + `QueryFullProcessImageNameW` собираются под MSVC | **подтверждено компиляцией** | изолированный scratch-крейт с `windows-sys 0.61` (features `Win32_System_Diagnostics_ToolHelp`, `Win32_System_Threading`) прошёл `cargo check`/`clippy -D warnings` под `x86_64-pc-windows-msvc`; в рантайме не запускалось |
| Linux: обход `/proc/<pid>/exe` компилируется под целевую тройку | **подтверждено компиляцией** | `rustc --target x86_64-unknown-linux-gnu` на изолированном модуле; в рантайме не запускалось |
| UI: загрузка, пусто, ошибка, поиск, добавление правила | **подтверждено тестами** | 6 сценариев `AppsScreen`; EN-прогон вкладки «Запущенные» без русского текста |
| Реальный список в окне `pnpm tauri dev` | **подтверждено частично** | окно `SplitRoute` открывается с MockHelper, процесс работает, после проверки остановлен; клик по вкладке в окне не автоматизировался — список проверен реальным сканом в тесте |

Решение и механизм:

- `RunningApp { pid, name, path, platform }` — в `crates/protocol`; UI получает
  готовый список от `apps_running` (`#[tauri::command(async)]`): обход
  процессов не выполняется на главном потоке окна.
- Платформенный обход отдаёт `ProcessEntry::{Found, Unreadable}`; фильтрация,
  дедупликация, сортировка и лимит — чистые функции
  (`src-tauri/src/catalog/running.rs`) и тестируются на macOS.
- `app_rule_from_running` создаёт выключенное правило: macOS — `path_regex` по
  `.app`, если путь внутри бандла, иначе `process_path`; Windows/Linux —
  `process_path`. Regex экранируется тем же помощником, что и каталог.
- Причины пропуска пишутся в Debug-лог (не больше 20 строк), системные и
  скрытые по лимиту записи — счётчиками. Ошибка чтения даёт пустой список и
  причину, остальные экраны работают.
- Иконки в «Запущенных» не запрашиваются: процессов сотни, а `app_icon`
  читает файл на каждый путь. Каталог с иконками остаётся на вкладке
  «Установленные» (S4-T2).

Ограничения и риски:

- **Правило по процессу, а не по «приложению».** В списке видны процессы —
  владельцы сокетов; `curl` из терминала и helper-процессы браузера ведут себя
  так же, как описано в PRD §6.3 и R2/R3. Для браузеров правило по `.app`
  снимает часть проблемы, но не превращает список в «приложения».
- **Linux/Windows в рантайме не проверены.** Компиляция подтверждает только
  корректность API; не проверены права доступа к чужим процессам, поведение
  kernel threads (у них нет `/proc/<pid>/exe`) и длина путей >260 символов на
  Windows.
- **Store/AppX и Flatpak/Snap** могут отдавать пути внутри sandbox или
  недоступные для чтения; такие записи либо пропускаются, либо требуют ручной
  проверки правила (PRD §6.3).
- **Лимит 500** осознанно скрывает хвост списка: на типичной машине после
  фильтра процессов меньше, но при сотнях пользовательских процессов часть
  записей не видна — в Debug-логе есть счётчик скрытых.
- **Нет автоматического обновления списка.** Он читается при открытии вкладки
  и по кнопке «Обновить список»; фоновый опрос не делается, чтобы не нагружать
  систему и не показывать устаревшие pid как актуальные.
- **macOS 26/27 (R1)** не проверялись: если process-матчинг в sing-box
  сломается, список процессов останется корректным, но правила могут не
  сработать — это риск ядра, а не каталога.

## 24. Пометка правил с несуществующим путём (проверено, 2026-11-14)

Подцель G4.10 (backlog B1, закрыта как S4-T12). Окружение: macOS 15.8.2 arm64,
обычный пользователь; проверка — локальный `stat`, без сети, helper'а и
привилегий. В рантайме на Linux/Windows не проверялось (там правила каталога
создаются как `process_path`, код проверки тот же).

| Утверждение | Статус | Как проверено |
|---|---|---|
| `missing_paths` можно считать на чтении и не хранить в `state.json` | **подтверждено тестами** | `AppRuleView` с `#[serde(flatten)]`; пустой вектор не сериализуется, `AppRule` игнорирует лишнее поле при чтении (UI отправляет правило обратно при переключении) |
| Абсолютный `process_path` проверяется `stat`-ом | **подтверждено тестами** | временный файл → активно; `/nonexistent/...` → он в `missing_paths`; относительный путь не проверяется |
| Regex каталога `^<escaped путь>/` сводится к литеральному пути | **подтверждено тестами** | `^/Applications/Google Chrome\.app/` → `/Applications/Google Chrome.app/`; экранированные спецсимволы разэкранируются |
| Regex с метасимволами не даёт ложного «неактивно» | **подтверждено тестами** | `.*`, `[A-Z]`, `(app|bak)`, `$`, отсутствие `^` и завершающего `/` → список пуст, правило считается активным |
| Правило с несколькими путями активно, если жив хотя бы один | **подтверждено тестами** | смешанный список → `missing_paths` пуст; все отсутствуют → перечислены все |
| `process_name` не проверяется | **подтверждено тестами** | правило только по имени процесса всегда активно: незапущенное приложение — не ошибка правила |
| `rules_list`/`rules_upsert`/`rules_delete` не блокируют главный поток | **подтверждено кодом** | все три — `#[tauri::command(async)]`; проверка путей идёт в thread pool Tauri |
| UI: бейдж, подсказка, включение | **подтверждено тестами** | 3 сценария `AppsScreen`: пометка и `title`, включение правила с отсутствующим путём, отсутствие пометки у regex-правила |

Решение и механизм:

- `AppRuleView { …AppRule, missing_paths }` — в `crates/protocol`; сохранённые
  данные по-прежнему только `AppRule`. Признак пересчитывается при каждом
  чтении: файл мог появиться или исчезнуть.
- Чистый модуль `src-tauri/src/rules.rs`: `missing_paths(rule, exists)` принимает
  предикат существования, поэтому тесты не зависят от файловой системы;
  `view`/`views` подставляют `Path::exists`.
- Поведение при сомнении — «правило активно»: пустой вектор означает и «всё
  хорошо», и «проверить нельзя».
- UI: бейдж «Неактивно» с `title`-подсказкой; включение и удаление правила не
  блокируются, `missing_paths` в `rules_upsert` не отправляется.

Ограничения и риски:

- **macOS-правила из каталога и «Запущенных» — это `path_regex`**, поэтому
  пометка появляется только для шаблонов, которые сводятся к литеральному пути
  (как раз то, что создаёт каталог). Regex с метасимволами остаётся без пометки
  осознанно: проверить его нельзя.
- **Путь может появиться после чтения списка.** Признак устаревает до
  следующего `rules_list`; фоновый опрос не делается.
- **Отсутствие запущенного процесса не проверяется.** Правило по имени процесса
  может быть активным, даже если приложение сейчас закрыто.
- **Сетевые и съёмные тома.** `stat` может отвечать медленно на отключённом
  сетевом томе; вызов вынесен в thread pool, но таймаута на отдельный `stat`
  нет.
- **Правила другой платформы** (например, `windows`-правило в `state.json` на
  macOS) не отбрасываются и не помечаются: их обработка — вне объёма B1.

---

## 25. Автоподключение при старте (`connect_on_start`) — B2, S4-T13 (проверено 2026-11-14)

Настройка `preferences.connect_on_start` сохранялась и отображалась в UI, но при
запуске приложения не применялась. Решение (PRD §6.5): применять её один раз
при старте, в фоне, тем же путём, что и кнопка «Подключить».

| Утверждение | Статус | Как проверено |
|---|---|---|
| Повторный `start` при уже запущенном ядре не нужен и отвергается helper'ом | **подтверждено кодом и тестом** | `crates/helper/src/core.rs` → `HelperError` `ALREADY_RUNNING`; решение пропускает попытку и тест `already_running_core_is_not_started_again` фиксирует `start_calls() == 1` |
| Без сохранённой локации попытка не выполняется и ошибки для UI нет | **подтверждено тестами** | `enabled_without_profile_is_skipped`, `stale_selection_is_treated_as_missing_profile`: `start_calls() == 0`, `startup_error()` пуст |
| Успешное автоподключение доходит до UI | **подтверждено в реальном окне** | `HOME=<изолированный> SPLITROUTE_MOCK_HELPER=1 pnpm tauri dev` с `connect_on_start: true` и синтетическим профилем: приложение открылось в состоянии «Подключено», время сессии шло с момента старта |
| Ошибка автоподключения — штатное состояние, окно работает | **подтверждено в реальном окне** | тот же запуск без `SPLITROUTE_MOCK_HELPER`: `ErrorBox` «Helper не установлен.» (`HELPER_NOT_INSTALLED`), доступны вкладки, кнопка «Подключить» и карточки статусов |
| Ошибка одного прохода не мешает следующему ручному подключению | **подтверждено тестом** | `start_failure_is_reported_and_app_stays_usable`: после `Failed` ручной `connect_inner` проходит |
| В лог не попадают адреса серверов | **подтверждено тестом** | `Warn`-строка «Автоподключение не удалось: HELPER_ERROR» без `203.0.113.*` (адрес профиля в фикстуре) |
| `launch_minimized` не ломается | **подтверждено кодом** | скрытие окна выполняется до `autoconnect::spawn`; автоподключение от видимости окна не зависит |

Механизм:

- `src-tauri/src/autoconnect.rs`: `run(&AppState) -> Outcome` — чистое решение и
  вызов общей `connect_inner`; `spawn(AppHandle)` запускает его через
  `tauri::async_runtime::spawn_blocking`, потому что IPC с helper'ом блокирующий
  (таймаут до 10 с), а окно ждать не должно. Публикация статуса в трей идёт
  через `run_on_main_thread`.
- Гейт: настройка → сохранённая локация (выбранный id присутствует в списке) →
  `helper.status()` не `Running`. Недоступный helper намеренно не выделен в
  отдельную ветку: `connect_inner` вернёт типизированную причину
  (`HELPER_NOT_INSTALLED` и т. п.), и пользователь увидит точный код.
- Ошибка хранится в `AppState.startup_error` и отдаётся командой
  `autoconnect_status`; поздний сбой приходит событием `autoconnect-failed`.
  Два канала нужны из-за порядка загрузки: автоподключение стартует в `setup`,
  до готовности webview, поэтому «ранний» сбой UI забирает чтением, «поздний» —
  событием. Чтобы не потерять событие, слушатели ставятся до `initialize()`
  (`App.tsx`). Ошибка снимается успешным `connect`/`disconnect`.
- UI-строки не добавлялись: используется существующий `ErrorBox` и коды
  `errors.*` из `ru`/`en` (PRD §6.5 не описывает отдельную строку состояния).

Ограничения и риски:

- **Helper недоступен → ошибка на «Главной» при каждом запуске.** Это осознанно:
  настройка включена пользователем, подключение не состоялось. Код
  `HELPER_NOT_INSTALLED`/`HELPER_UNAVAILABLE` уже переводится, причину видно и в
  логах.
- **Single instance.** Если второй экземпляр успевает выполнить `setup` до
  выхода, оба могут пройти проверку «ядро не запущено». Второй `START` helper
  отвергает (`ALREADY_RUNNING`), поэтому дубля ядра не будет, но в UI может
  появиться предупреждение. Гонка не устранена — вынесена в ограничения.
- **Порядок запуска.** Автоподключение начинается в `setup`, до загрузки
  webview; окно появляется раньше, чем приходит результат. Это ожидаемо и
  отражено в PRD §6.5.
- **Linux/Windows-рантайм и установленный реальный helper** не проверялись: код
  платформонезависим, но поведение launchd/systemd/службы Windows в этом
  сценарии не наблюдалось.

---

## 26. Доменные правила провайдера — решение A (2026-11-14)

Вопрос PRD §13.2 закрыт вариантом A: правила из шаблона провайдера в
генерируемый конфиг не переносятся. PRD §13 п. 7, задача S4-T7 закрыта без
изменений кода.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Шаблон провайдера содержит доменные правила, а не только outbound'ы | **подтверждено** | обезличенное реальное тело: `spike/fixtures/subscription.json` → `routing.rules` c `geosite:private`, `geosite:category-ads-all` и правилом в `fragment-out` |
| Наш генератор эти правила не использует | **подтверждено** | поиск по `src-tauri/src`: нет `routing`, `rule_set`, `geosite`; маршрут строится только из `AppRule` и режима DNS |
| В sing-box 1.14 legacy `geoip`/`geosite` в правилах недоступны | **подтверждено** | migration guide, раздел 1.8.0 «Migrate GeoIP/Geosite to rule-sets»: вместо ключей — `rule_set` с remote-SRS (`format: "binary"`, URL `raw.githubusercontent.com/SagerNet/sing-geoip|sing-geosite`) и обязательный `cache_file`; ядро закреплено на 1.14.4 (`scripts/core-version.txt`) |

Почему не переносим:

1. **Конфликт с обещанием продукта.** SplitRoute обещает «выбранное приложение
   целиком идёт через прокси». Доменные правила выполняются раньше
   process-правил, поэтому трафик выбранного приложения к российским IP и
   direct-доменам уходил бы напрямую — мимо туннеля, внутри «защищённого»
   приложения. Это и утечка, и необъяснимое поведение в UI.
2. **Новая сетевая зависимость.** SRS-файлы нужно скачивать и кэшировать;
   появляется поведение без сети, обновление списков и лишняя точка отказа на
   старте.
3. **Стоимость в UI и тестах.** Нужен переключатель, предупреждение, объяснение
   приоритета правил и golden-тесты на порядок — это отдельная фича, а не
   деталь конвертера.

Практические следствия:

- трафик выбранных приложений целиком идёт в прокси, включая сайты, которые
  шаблон провайдера отправлял напрямую;
- блокировка рекламы и direct-домены провайдера не действуют — это записано в
  PRD как «Could have», а не как дефект;
- если фича понадобится после MVP: только `rule_set` с локальными SRS (без
  скачивания на старте), явный переключатель профиля с предупреждением, приоритет
  правил зафиксирован golden-тестами и проверен `sing-box check`.

---

## 27. CI-сборка Windows и Linux (проверено 2026-11-14)

Репозиторий — `github.com/tsybindev/splitroute` (публичный), workflow —
`.github/workflows/build.yml`.

| Что | Результат |
|---|---|
| Матрица | `ubuntu-22.04` и `windows-latest`; macOS в CI нет |
| Linux-пакеты | `SplitRoute_0.1.0_amd64.deb` (34 МиБ), `SplitRoute-0.1.0-1.x86_64.rpm` (34 МиБ), `SplitRoute_0.1.0_amd64.AppImage` (105 МиБ) |
| Windows-пакет | `SplitRoute_0.1.0_x64-setup.exe` (NSIS, 23 МиБ) |
| Тесты | `cargo test --workspace --locked`: 377 на Unix, 241 на Windows (Unix-only под `cfg(unix)`); фронтенд — 96 |
| Длительность | 5–6 мин с кэшем Rust/pnpm, до 16 мин на холодную (первый release-билд) |

Что вскрыл первый прогон (всё исправлено отдельными коммитами):

1. **`privileged::linux` не собирался на Windows.** Модуль компилируется с
   `cfg(test)` на любой ОС ради тестов генерации юнита, но напрямую использовал
   `std::os::unix::fs::PermissionsExt` (E0433/E0599). Права теперь выставляет
   локальный `set_mode`: на Unix — как раньше, на остальных ОС — no-op.
2. **`target/` в корне workspace.** `tauri build` пишет пакеты в
   `target/release/bundle`, а не `src-tauri/target/release/bundle`.
3. **CRLF в checkout.** Три golden-теста сравнивают JSON побайтово и падали
   только из-за `\r\n`. Добавлен `.gitattributes` (`* text=auto eol=lf`) — заодно
   CRLF больше не попадёт в shell-скрипты.
4. **Тесты, завязанные на Unix.** Литералы `/opt/...`, `/Applications/...`,
   `.desktop` с Unix-Exec и `sh -n` заменены на пути из `temp_dir()` либо
   помечены `cfg(unix)`; на Windows закрытый loopback-порт может ответить
   таймаутом, а не RST — тест принимает оба сетевых кода.
5. **Артефакт AppImage тянул промежуточный `SplitRoute.AppDir`** (и конфликтовал
   именами при распаковке на регистронезависимой ФС) — в артефакт попадают
   только `.deb`/`.rpm`/`.AppImage`/`.exe`.

Ограничения:

- Пакеты **не подписаны** (`SPLITROUTE_ALLOW_UNSIGNED=1`), updater-артефактов
  нет; SmartScreen и Gatekeeper будут предупреждать.
- CI проверяет сборку, unit-тесты и содержимое бандла. TUN, установка
  helper-сервиса, per-app-маршрутизация и DNS не проверяются — ручной smoke
  (S3-T7) остаётся blocked.
- Ветка `main` собирается и на push, и на pull request; macOS-job не добавлен,
  потому что подпись/нотаризация не настроены, а `.app`/`.dmg` собираются
  локально (README).

## 28. Автообновление подписки по интервалу (проверено 2026-11-14)

Реализация: `src-tauri/src/autoupdate.rs`, `AppState` в
`src-tauri/src/commands/mod.rs`, индикация — `src/features/profiles/ProfilesScreen.tsx`.
URL подписки и HWID в логах и отчёте не выводятся.

| Утверждение | Статус | Как проверено |
|---|---|---|
| Расписание считается от последней успешной загрузки и последней попытки (что позже) | **подтверждено тестом** | `next_update_at`/`base_time`/`is_due`: `next_update_is_counted_from_last_success`, `later_attempt_wins_over_older_success`, `without_any_data_first_update_is_due` |
| Интервал `0` выключает автообновление, сеть не трогается | **подтверждено тестом** | `zero_hours_disables_auto_update`, `disabled_interval_does_not_touch_network`: 0 запросов, `next_update_at() == None` |
| Пустой URL не запускает загрузку | **подтверждено тестом** | `empty_url_does_not_touch_network`: `Outcome::NoUrl`, 0 запросов |
| Свежая подписка не перезапрашивается | **подтверждено тестом и live** | `fresh_subscription_is_not_fetched_again`; live-прогон с `fetched_at = now-60` и интервалом 12 ч — **0 запросов** к локальному серверу за 18 с |
| Истёкший интервал обновляет подписку и `profiles_fetched_at` | **подтверждено live** | `pnpm tauri dev` (MockHelper) + локальный HTTP-сервер: `fetched_at` 1700000000 → текущее время, 2 профиля в кэше, ровно **1 запрос** |
| Ошибка сети сохраняет кэш и не стирает выбор локации | **подтверждено тестом** | `failure_keeps_cache_and_postpones_the_next_attempt`: профили и `profiles_fetched_at` прежние, `WARN` без адресов серверов |
| Сломанный провайдер не опрашивается на каждом тике | **подтверждено тестом** | тот же тест: после ошибки попытка запоминается, следующий тик `NotDue`, запросов остаётся 1 |
| Параллельные загрузки исключены | **подтверждено тестом** | `parallel_fetch_is_rejected`: `try_begin_fetch` держит флаг; планировщик получает `Outcome::Busy`, команда — `FETCH_IN_PROGRESS`, сеть не вызывается |
| Флаг загрузки снимается при любом исходе | **подтверждено кодом и тестом** | `FetchGuard: Drop`; после `drop` следующий `run` проходит успешно |
| Загрузка не блокирует окно | **подтверждено кодом** | планировщик — `tauri::async_runtime::spawn_blocking`, ручная команда остаётся `#[tauri::command(async)]` (S4-T9) |
| Новых зависимостей нет | **подтверждено** | `Cargo.toml`/`Cargo.lock` не менялись: `tauri::async_runtime` + `std::thread` |
| UI показывает «следующее обновление» и «автообновление выключено» | **подтверждено тестами** | 2 UI-теста `ProfilesScreen` (время следующего обновления; интервал 0) |
| Фоновое обновление доезжает до UI без перезагрузки окна | **подтверждено кодом** | событие `subscription-updated` → `onSubscriptionUpdated` в `appStore.subscribe` (ставится до `initialize`) |
| Linux/Windows-рантайм | **не проверено** | нет машин; код платформонезависим, helper и протокол не менялись |

Ограничения:

- Планировщик живёт только пока запущено приложение: закрытое приложение
  подписку не обновляет. Отдельной фоновой службы нет и в MVP не планируется.
- Время последней попытки хранится в памяти, поэтому после перезапуска
  отсчёт идёт от `profiles_fetched_at`; если подписка была неудачной и стара,
  первая проверка после старта попробует снова.
- Проверка расписания — раз в минуту, поэтому фактический момент обновления
  может сдвинуться вперёд до 60 с.
- Ручное обновление и планировщик делят один флаг: если планировщик уже
  грузит, кнопка «Обновить подписку» вернёт `FETCH_IN_PROGRESS`, а UI это не
  показывает как ошибку (локации обновятся событием).

## 29. Несколько подписок одновременно — этап 6 (реализовано 2026-11-19)

Коммит `f824f82`. Backlog-задача «Несколько подписок одновременно» (PRD §13 п. 8).

### Модель

- `state.json` вместо плоских `profiles`/`profiles_fetched_at` и URL в
  `settings.subscription` хранит список записей `subscriptions`:
  `{id, name, url, user_agent, auto_update_hours, profiles, fetched_at}`.
  `settings.subscription` оставлен только с `hwid` — общий идентификатор
  устройства для всех провайдеров.
- `id` записи — `uuid::Uuid::new_v4().simple()`, генерируется при создании.
  Миграция старого файла создаёт запись и **сразу пишет файл на диск**:
  иначе идентификатор менялся бы при каждом запуске (миграция детерминирована
  условием «`subscriptions` пуст и в legacy-настройках есть непустой URL»).
- Активная локация осталась плоской (`selected_profile_id`): идентификатор
  профиля вычисляется из содержимого (имя, сервер, порт, UUID), поэтому
  коллизия между подписками означает буквально одинаковые профили — тогда
  они и ведут себя одинаково. Отдельная пара «(подписка, профиль)» признана
  избыточной.

### Расписание и параллелизм

- Расписание у каждой записи: `max(fetched_at, последняя попытка) + интервал`.
  Попытка запоминается в `subscription_fetch_inner` (общий путь ручного
  обновления и планировщика), поэтому **неудачная ручная попытка тоже
  переносит расписание** — в предыдущей реализации это было справедливо
  только для планировщика (расхождение с текстом PRD §6.1 устранено).
- Guard загрузки — не общий флаг, а `Mutex<HashSet<String>>` с id подписки:
  ручное обновление одной подписки не конфликтует с фоновым обновлением
  другой (раньше планировщик занимал единственный флаг и ручное обновление
  любой подписки молча отбрасывалось как `FETCH_IN_PROGRESS`).
- Планировщик обходит все подписки последовательно на каждом тике; событие
  `subscription-updated` теперь несёт данные одной подписки
  (`subscription_id` в payload).

### Проверено

- 397 Rust- и 103 frontend-теста; fmt/clippy/typecheck/lint/build чисто;
  `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc` чисто.
- Live-проверка (MockHelper + изолированный `HOME` + локальный HTTP-сервер
  с фикстурой base64/vless): legacy `state.json` с URL/UA/интервалом и
  `profiles_fetched_at` в прошлом → при запуске мигрирован в первую запись
  (legacy-поля из файла удалены), планировщик сделал **ровно 1 запрос** с
  ожидаемыми `User-Agent` и `X-HWID`, 1 профиль закэширован в запись,
  `fetched_at` обновлён, локация выбрана автоматически; второй тик через
  70 с не сделал новых запросов. Процессы остановлены, порт 1420 свободен.
- Новые команды: `subscription_fetch(id)`, `subscriptions_list`,
  `subscription_add`, `subscription_update` (смена URL сбрасывает кэш
  записи), `subscription_delete` (возвращает общий список — выбор мог
  сброситься). `profiles_list` отдаёт `subscriptions` (группы с расписанием)
  и плоский `profiles`.

### Ограничения и не проверено

- Клик-проверки UI (группы «Локаций», карта «Подписок») не автоматизировались:
  покрыты тестами Vitest и проверкой на уровне стора/команд.
- Импорт существующей подписки повторным добавлением того же URL создаёт
  вторую запись с теми же профилями: выбор остаётся валидным, но локации
  отображаются дважды. Дедупликация по URL — не делалась (пользовательский
  сценарий «две записи с одним URL» не считается ошибкой).
- Лимит 64 профиля на проверку задержек общий на все подписки (суммарно), как
  и раньше.
- Linux/Windows-рантайм — без изменений (см. §27 и blocked-пункты этапов 3–4).

## 30. Clash YAML как формат подписки — этап 7 (реализовано 2026-11-20)

Коммит `ceca190`. Backlog-задача «Clash YAML» (PRD §13 п. 9). Helper и его
протокол не менялись.

### Формат и детект

- Тело Clash-подписки — YAML-отображение, узлы лежат в ключе `proxies`
  (`{name, type, server, port, …}`). Детект встроен в существующую цепочку
  `parse_subscription_report`: JSON → vless-текст → **Clash** → base64 →
  `INVALID_FORMAT`. Тело считается Clash-конфигом, если это
  YAML-отображение с ключом `proxies`; иначе цепочка пробует следующий
  формат (`try_parse` возвращает `Ok(None)`).
- YAML — надмножество JSON, поэтому: JSON-объект с `proxies` принимается
  как Clash в JSON-нотации (проверка после удачного `serde_json::from_str`);
  фигурные скобки без кавычек ключей (`{port: 7890, …}`) — не JSON, но
  валидный YAML flow-стиль, такие тела разбираются фолбэком после ошибки
  serde_json (ветка в `parse_json_report`).
- Валидный YAML без ключа `proxies` (случайный текст, скаляр, документ
  без узлов) не считается ошибкой формата на этапе детекта — цепочка
  пробует base64. Сломанный YAML с `proxies` (обрыв документа) отдаёт общий
  `INVALID_FORMAT`, а не текст ошибки YAML: специального «похоже на
  Clash»-преддетекта не делалось.
- Ключ `proxies` есть, но не список (`proxies: 5`) → `INVALID_FORMAT`;
  `proxies: []` → `EMPTY` (как пустой JSON-массив у Happ).

### Маппинг узла (mihomo/Clash.Meta → `Profile`)

- Конвертируются только узлы `type: vless`; остальные (`ss`, `ssr`, `vmess`,
  `trojan`, `hysteria2`, `tuic`, `wireguard`, `socks5`, `http`, `anytls`, …)
  пропускаются с причиной `type=… не поддерживается` — модель профиля MVP
  содержит только VLESS-параметры, расширение потребовало бы новых
  протоколов в ядре и генерации outbound'ов sing-box (PRD §13 п. 9).
- Reality — по наличию `reality-opts.public-key` (независимо от флага
  `tls`); `servername` → SNI (по умолчанию адрес сервера),
  `client-fingerprint` → fingerprint, `reality-opts.short-id` → short id.
  Обычный TLS — по `tls: true`, `alpn` — YAML-последовательность строк.
  `tls: false` или отсутствие обоих → скип (`tls=false не поддерживается`,
  аналог `security=none` в vless-ссылках).
- `network: ws/grpc/…` → скип (`network=… не поддерживается`);
  `skip-cert-verify: true` → скип — модель и генерация конфига не
  поддерживают insecure-TLS, узел бы не подключился. Неизвестные поля
  (`udp`, `packet-encoding`, `ws-opts`, …) игнорируются, парсер не падает.
- Порт — строго `as_u64` (как в Happ-парсере), строковый `port: "443"` не
  принимается. SNI читается только из `servername` (ключ `server-name` не
  поддерживается). `raw` узла кладётся в кэш конвертацией YAML→JSON
  (`serde_json::to_value(item).ok()`), в UI вырезается как и раньше.

### Зависимость

- `serde_yaml = "0.9"` (workspace + src-tauri): зрелый минимальный
  serde-парсер; крейт в archive-статусе, но API заморожен и стабилен, а
  форки (`serde_yml`) менее проверены. Обоснование — комментарий в
  `src-tauri/Cargo.toml` и PRD §13 п. 9. Лимита тела подписки нет (как и
  для других форматов) — ограничением остаются таймауты 45/10 с;
  Clash-конфиги бывают большими, но тело и так читается целиком.

### Проверено

- 413 Rust- (+16) и 103 frontend-теста; fmt/clippy/typecheck/lint/build
  чисто; `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc`
  чисто. End-to-end тест fetch: HTTP-сервер отдаёт Clash YAML → 1 профиль
  Reality + 1 скип vmess (`fetch.rs::clash_yaml_is_parsed_end_to_end`).
- Live-проверка (MockHelper + изолированный `HOME` + локальный HTTP-сервер
  с Clash-фикстурой, python-оркестратор по шаблону §29): seeded
  `state.json` с подпиской (UA `ClashVergeTest/1.0`, интервал 1 ч, пустой
  кэш) → при запуске планировщик сделал **ровно 1 запрос** с ожидаемыми
  `User-Agent` и `X-HWID`, vless-узел разобран в кэш записи со всеми
  Reality-полями, `fetched_at` обновлён, vmess-узел пропущен; через 70 с
  повторных запросов нет. Процессы остановлены, порты 1420/18081 свободны,
  временный каталог удалён.

### Ограничения

- `proxy-providers` (внешние списки узлов внутри Clash-конфига) не
  поддерживаются — только inline `proxies`.
- Дубликаты узлов не дедуплицируются (как и повторное добавление того же
  URL — §29): совпадение профиля между подписками означает буквально
  одинаковые узлы, поведение идентично.
- Порядок веток детекта означает: тело, начинающееся с `[`/`{` и не
  являющееся ни Happ-JSON, ни Clash-объектом, отдаёт `InvalidJson`, а не
  `InvalidFormat` (как и раньше).

## 31. Kill switch под root на macOS — S4-T5c (проверено 2026-11-20)

Первая рантайм-проверка kill switch с реальным профилем и root. Коммиты
фиксов: `c3aa3ca` (правила pf), `a27161b` (blocking-команды в
`spawn_blocking`). Helper и протокол не менялись (правка — только текст
правил внутри helper'а).

### Как проверять без подписи Apple (обход S3-T2)

- Dev-helper запускается вручную под `sudo` вместо LaunchDaemon:
  `sudo target/debug/splitroute-helper --socket /tmp/splitroute-dev.sock
  --runtime-dir /tmp/splitroute-dev-runtime --sing-box
  <abs путь к src-tauri/binaries/sing-box-aarch64-apple-darwin>`.
  Под sudo `SUDO_UID` подхватывается автоматически: сокет после bind
  `0600` + `chown` на uid пользователя — непривилегированное приложение
  подключается, peer-проверка пропускает.
- Приложение: `SPLITROUTE_HELPER_SOCKET=/tmp/splitroute-dev.sock
  pnpm tauri dev` (без `SPLITROUTE_MOCK_HELPER` — нужен настоящий helper).
- Прямой пробой протокола удобно сверять состояние helper'а независимо от
  UI: newline-JSON `{"protocol":2,"id":"…","command":"status"}` в сокет,
  ответ — одна строка JSON (`state`, `pid`, `kill_switch.enabled`).

### Найдено и исправлено

1. **Правила pf не загружались — `KILL_SWITCH_FAILED` при каждом
   подключении с kill switch.** Причина: правило
   `pass out quick user root all keep state` нарушает порядок pf-грамматики
   (man pf.conf: `hosts` — до `filteropt`; пример в man:
   `pass out proto { tcp, udp } all user { … }`). pfctl целиком отбивал
   файл: `killswitch.pf:5: syntax error` → helper корректно откатывал и
   `pfctl -E`-токен (pf возвращался в Disabled — откат проверен живьём).
   Фикс: `pass out quick all user root keep state` (`c3aa3ca`), тест правил
   обновлён. Существующие тесты закодировали тот же неверный порядок —
   их не ловили, потому что текст правил синтаксисом не проверялся.
2. **Вечный спиннер «Обновить» и мёртвый UI.** Команды `subscription_fetch`
   и `traffic_stats` были синхронными телами с `#[tauri::command(async)]`:
   Tauri исполняет их в таске на воркере tokio, а blocking-reqwest в
   debug-сборке при каждом вызове строит и тут же дропает свой
   current-thread runtime (`reqwest 0.13 blocking/wait.rs::enter` — guard
   «we aren't already in a runtime»); дроп runtime из async-контекста —
   паника tokio (`Cannot drop a runtime in a context where blocking is not
   allowed`). Таска команды умирала → промис в UI не разрешался никогда:
   спиннер крутится, кнопка не нажимается, `FETCH_IN_PROGRESS` не при чём.
   В release-сборках guard вырезан `debug_assertions` — поэтому баг жил
   только в dev. Планировщики (`autoupdate`/`autoconnect`) не страдали —
   они уже на `spawn_blocking` (у blocking-потока нет runtime-контекста).
   Фикс: обе команды переведены в `async fn` + `spawn_blocking`
   через `AppHandle` (`a27161b`), защитный код ошибки `FETCH_ABORTED`.
   Правило на будущее: **новая blocking-работа в командах — только через
   `spawn_blocking`; `#[tauri::command(async)]` на синхронном теле означает
   «исполнять на воркере tokio», а не «на thread pool»**.

### Подтверждено в рантайме (реальный профиль, helper под root)

- Подключение через pf-анкер `com.apple/splitroute.killswitch`: правила
  применяются (текст разворачивается в 11 правил), `/etc/pf.conf` не
  меняется, токен в runtime-каталоге.
- Прямой (не туннелированный) egress обычного пользователя блокируется
  (`curl` — timeout), локальная сеть и loopback работают — защита шире
  allowlist, как и задумано (§20).
- Реальное убийство ядра: `pkill -x sing-box-aarch64-apple-darwin` →
  helper репит процесс, `state: stopped`, **правила остаются**, прямой
  egress продолжает блокироваться (fail-closed); UI показывает
  `CORE_STOPPED` через ~2 с (опрос `helperStatus` в `ConnectScreen`,
  ветка `protectedCrash`). После отключения: anchor not found, прямой
  трафик восстановлен, pf Disabled.
- Откат при неудачном старте не оставляет включённый pf: после
  `KILL_SWITCH_FAILED` (до фикса правил) pf вернулся в Disabled сам.

### Грабли и ограничения

- **`pkill -x sing-box` не убивает ядро**: имя процесса обрезается ядром
  до 16 символов (`sing-box-aarch64`, comm/MAXCOMLEN), `-x` требует точного
  совпадения. Убивать по полному имени бинаря
  (`pkill -x sing-box-aarch64-apple-darwin`) или по PID из статуса helper'а.
  Из-за этого первые две попытки сценария «падение ядра» были ложными
  (ядро продолжало жить, UI честно показывал «Подключено»).
- Статистика (Clash API) первые ~2 с после подключения недоступна (ядро
  ещё поднимается) — один `WARN`, дальше работает; после смерти ядра —
  «—» с одним `WARN`, как и зафиксировано в §19.
- Детект падения есть, **автопереподключения нет** (PRD §13 п. 4 обещает
  до 3 попыток) — задача `S4-T15`.
- Windows-часть S4-T5c: функционал проверен пользователем на реальной
  машине; формальные WFP-пункты (перечисление фильтров, блокировка
  `curl.exe` при мёртвом ядре, снятие после `stop`) — по детальному
  отчёту. Linux — без root, не проверялся.

## 32. Автореконнект ядра — S4-T15 (реализовано и проверено 2026-11-20)

Детект падения перенесён из UI в Rust: фоновый надзор `src-tauri/src/reconnect.rs`
запускается в `setup` рядом с autoconnect/autoupdate. Коммит `57158a3`.

### Решение

- **Где живёт детект.** Надзор при активной сессии (`watching`) опрашивает
  `helper.status()` раз в 2 с. Пока сессии нет — цикл спит без IPC. Работает
  без kill switch и при закрытом окне (tray-only), в отличие от прежнего
  фронтового опроса, который включался только при активном kill switch.
  Ошибка статуса helper'а падением ядра не считается: серия запускается
  только по явному `state: stopped` от живого helper'а.
- **Серия.** До 3 попыток (`MAX_ATTEMPTS`) через общий `connect_inner` —
  без дублирования логики кнопки. Пауза между попытками 3 с, спит чанками
  по 200 мс с проверкой отмены: ручное отключение прерывает серию, не дожидаясь
  паузы. Перед каждой попыткой статус перепроверяется: если ядро уже поднялось
  вручную (пользователь успел во время серии), серия завершается как успешная
  без лишнего `start` (helper вернул бы `ALREADY_RUNNING`).
- **Счётчик и исчерпание.** Успешный старт (`connect_inner` → `reconnect_arm`)
  сбрасывает серию; ручное отключение (`disconnect_inner`), переключение
  локации и stop перед установкой обновления — `reconnect_disarm` (наблюдение
  закрывается до `helper.stop()`, чтобы серия увидела отмену раньше).
  После 3 неудач — `exhausted` в `AppState`, событие `reconnect-failed`
  (payload `CommandError{code:"RECONNECT_FAILED"}`), повторная серия не
  запускается до ручного действия. Guard серии (`running` + Drop) исключает
  параллельные серии и снимается при любом выходе — регресс-тест на это
  поймал реальный баг (первая версия не сбрасывала `running`, и серия после
  отмены больше не запускалась бы).
- **UI.** События `reconnect-progress` (перед каждой попыткой) и
  `reconnect-failed`; команда `reconnect_status` читается при старте окна —
  серия могла идти или исчерпаться при закрытом окне (аналог B2).
  Бейдж «Автореконнект: попытка N из 3» на «Главной»; при
  `connection-changed` со `running` ошибки `CORE_STOPPED`/`RECONNECT_FAILED`
  снимаются. Строки `ru`/`en`.
- **Fail-closed.** Серия не вызывает `stop`: правила kill switch между
  попытками и при исчерпании остаются (снимает их только ручное отключение).
  В рантайме проверялся с kill switch выключенным; сохранение правил —
  юнит-тесты + живая проверка S4-T5c (§31).

### Live-проверка (helper под sudo, изолированный стор, kill switch off)

- Изолированный стор: `SPLITROUTE_STATE=/tmp/sr-live-state.json` (копия
  реального `state.json` с `connect_on_start=true`, `kill_switch=false`;
  секреты не печатались). Подключение через автоподключение при запуске —
  `reconnect_arm` в общем пути сработал сам.
- **Восстановление:** `stop` ядра командой в сокет helper'а — для надзора
  неотличимо от падения: через ~2 с ядро живо с новым PID (попытка 1).
  Повторный stop — снова ~2 с: счётчик и guard сбрасываются, серия
  запускается заново.
- **Исчерпание:** подмена бинаря заглушкой `exit 1` (`sing-box check`
  падает) + stop → 3 попытки с паузами (~10 с суммарно), красный
  `RECONNECT_FAILED` в UI (подтверждено пользователем), дальше ретраев нет
  (наблюдение 24 с — состояние `stopped` стабильно).
- **Ручное действие:** после восстановления бинаря «Подключить» подняло
  ядро (сброс исчерпания), «Отключить» — чистое завершение; бейдж серии
  пользователь подтвердил.
- Убить root-процесс ядра из непривилегированной сессии нельзя
  (`kill -9` — operation not permitted); `stop` в сокет — рабочий
  обход для сценария «падение» без sudo-kill.

### Грабли и ограничения

- **Переопределение `HOME` ломает окно Tauri.** Первая попытка live-проверки
  запускала приложение с `HOME=/tmp/…`: webview (WKWebView) не поднялся —
  окно открывалось полностью белым (только заголовок), при этом фронтенд
  в обычном браузере рендерился. Решение — `SPLITROUTE_STATE` (env-путь
  стора по образцу `SPLITROUTE_HELPER_SOCKET`), `HOME` не трогать.
- **Kill switch + автоподключение блокирует всю машину при мёртвом туннеле**
  (ожидаемое поведение, R14): если туннель не проходит трафик, fail-closed
  оставляет без сети все непривилегированные процессы, включая агентную
  сессию. Для live-проверок автореконнекта kill switch выключать.
- Убийство ядра по PID требует root (процесс принадлежит helper'у под sudo);
  `stop` через сокет — эквивалентный по наблюдаемому состоянию способ.
- Ретраи серии ограничены 3 попытками сознательно: сервер может быть
  недоступен долго, а бесконечные попытки мешали бы ручному переключению
  локации. После исчерпания пользователь действует сам — серия не мешает.

## 33. QR-импорт подписки — этап 8 (реализовано 2026-11-20)

Подписка добавляется из QR-кода: файл, drag-drop в окно или вставка
скриншота (Cmd+V). Декодирование локальное в Rust; цепочка детекта форматов
подписки (`subscription/`) не тронута — QR-текст не парсится как подписка,
из него извлекается только ссылка, дальше штатный `subscription_add` +
загрузка. Коммит `cf385dc`.

### Решение

- **Декодер — `rqrr` 0.11** (pure Rust). Ключевой пункт совместимости: его
  опциональная зависимость `image` допускает 0.25 (`cargo tree -i image` —
  одна версия в дереве, общая с tauri/image-png), поэтому
  `image::GrayImage` из нашего крейта передаётся напрямую
  (`PreparedImage::prepare(to_luma8())` → `detect_grids()` → `decode()`).
  У `rqrr` она включена фичей `img` по умолчанию; запасной вариант —
  `quircs` (binding к C, luma-буфер без привязки к версии image), не
  понадобился. JS-декодер во фронтенде отклонён: форматы и парсинг в
  проекте живут в Rust, а canvas в jsdom не тестируется.
- **Скоуз — только ссылка на подписку.** Модель записи требует URL
  (`add_subscription` отклоняет пустой → `InvalidUrl`), поэтому распознаются
  `http(s)`-ссылка с непустым хостом и Happ-обёртки
  `happ://add?url=…` / `happ://add/…` (значение — сначала base64
  standard/url-safe с паддингом или без — переиспользован декодер
  `subscription/base64.rs`, затем percent-decode). QR с `vless://`-ссылками
  даёт ошибку `errors.QR_NODE_LINKS` («импорт узлов без подписки пока не
  поддерживается»), нераспознанный текст — `errors.QR_NO_SUBSCRIPTION`;
  статичные записи без URL потребовали бы правок стора, планировщика и
  PRD — это отдельная фича, не делалась. Роутинг нескольких кодов на одной
  картинке: первый `url`/`happ` в порядке обнаружения декодером; если
  прямых ссылок нет, но есть `vless://` — `node_links`.
- **Команды.** `qr_decode_image(data_base64)` — тело картинки в base64
  (`btoa` вебвью, стандартный алфавит с паддингом); `qr_decode_file(path)` —
  путь от drag-drop. Обе — `spawn_blocking` (rqrr на больших картинках —
  десятки миллисекунд, fs::read — диск), ошибки —
  `QR_DECODE_FAILED` (не base64/не картинка/QR не найден/гибель таска) и
  `QR_FILE_UNREADABLE` (файл не открылся). Ответ
  `QrDecodeResponse{kind, url}` — snake_case, как у соседних структур.
  **Декодированный текст нигде не логируется** (URL — секрет по PRD §6.1).
- **UI.** `QrImportDialog`: оверлей с drop-зоной, кнопка выбора файла
  (`accept="image/png,image/jpeg"` — WebP/HEIC сознательно не
  поддерживаются, `image` без их фич), busy-состояние, ошибки в `ErrorBox`.
  Успех (`url`/`happ`) закрывает диалог и открывает обычную форму
  добавления с префиллом `{...emptyInput, url}` — один путь добавления.
  Кнопка «Импорт по QR» рядом с «Новая подписка», когда форма закрыта.
- **Источники картинки.** Drag-drop — через оконное событие Tauri
  `getCurrentWebviewWindow().onDragDropEvent` (`enter`/`over` → подсветка,
  `drop` → `paths[0]` → `qr_decode_file`): при включённом по умолчанию
  `dragDropEnabled` HTML5-события drop до страницы не доходят, поэтому
  подписка именно на событие Tauri (без правок конфига окна и
  capabilities). Paste — document-слушатель `paste` с
  `clipboardData.items` (первый `image/png|jpeg` → `getAsFile` →
  FileReader.readAsDataURL → срез префикса `data:…;base64,`); слушатели
  ставятся на время открытого диалога и снимаются при закрытии.
- **Отмена.** Токен декодирования в диалоге: «Отмена» инкрементирует его,
  и запоздавший результат invoke не открывает форму. Ревью поймало дыру в
  первой версии: `FileReader.onload`, завершившийся после отмены, получал
  свежий токен внутри `decode()` и проходил проверку — фикс одной
  проверкой токена в `onload` + регресс-тест (управляемый `FileReader`).
- **Тесты декодера — hermetic.** Dev-зависимость `qrcode 0.14.1`
  (`default-features = false`: без фичи `image`, у 0.15 сломан docs.rs):
  тест рендерит QR в пиксели (`to_colors()`, тихая зона 4, масштаб ×6),
  PNG кодирует штатный `image` — версии image крейтов не пересекаются,
  граница только PNG-байты. Коммитить PNG-фикстуры или генерировать их
  npx/python не пришлось. Multi-QR: rqrr находит оба кода на склеенной
  картинке (тест на 2 текста).

### Проверки

- 443 Rust-теста (20 новых: 13 роутинг/percent-decode, 7 decode_image —
  короткий/длинный URL, vless, multi-QR, битые байты, картинка без QR,
  связка decode+route) и 114 frontend (6 новых), fmt/clippy/typecheck/
  lint/build, `cargo check -p splitroute-helper --target x86_64-pc-windows-msvc`
  — чисто.
- THIRD_PARTY_NOTICES: секция 5 — +`g2p*`, `qrcode`, `zune-core`,
  `zune-jpeg` (MIT OR Apache-2.0), `lru` (MIT), `rqrr` в группу ISC с
  аннотацией `(MIT OR Apache-2.0) AND ISC`; снимок обновлён (536 пакетов).
  Тест `packaging_config` зелёный.
- Helper и его протокол не менялись; новых плагинов и правок capabilities
  нет (команды приложения в Tauri 2 разрешений не требуют).

### Грабли и ограничения

- **happ path-форма и `/`.** Значение `happ://add/<…>` берётся до `?`/`#`,
  не по `/`: внутри значения встречается standard-base64, в алфавите
  которого есть `/`. По той же причине `+` в percent-decode не превращается
  в пробел.
- **percent-decode продублирован** в `qr.rs` (аналог приватного в
  `vless.rs`): модуль `subscription/` менять запрещено контрактом этапа.
- **Порядок текстов** multi-QR — как вернул rqrr (порядок поиска
  capstone'ов, примерно слева-направо/сверху-вниз); контракт «первый
  url/happ выигрывает» делает результат детерминированным.
- **WKWebView paste** не проверялся живьём: скриншоты из Finder/Preview
  должны приходить как `image/png`, но mime иногда отличается — если
  вставка не сработает, остаётся выбор файла и drag-drop (см.
  NEXT_SESSION.md — live-проверка).
- **Декодер адаптивно бинаризует** изображение: на реальных скриншотах с
  низким контрастом/масштабом возможны неуверенные распознавания —
  сообщение `QR_DECODE_FAILED` просит переснять скриншот целиком.
- **Осталось:** live-проверка с реальным QR из окна приложения
  (изолированный стор, kill switch выключен), Linux/Windows-рантайм.

## 34. Linux-рантайм на CachyOS по SSH — S2-T9/S3-T7 (проверено 2026-11-21)

Live-проверка Linux-части по `docs/linux-runbook.md`: CachyOS по SSH
(denis@192.168.0.49, LAN), агент-оркестратор на macOS, sudo-команды
пользователем. Кодовая база переносилась rsync'ом (origin/main отставал
на 14 коммитов; дерево рабочее, включая `.git`).

### Среда

- CachyOS x86_64, kernel 7.3.4-1-cachyos, rustc/cargo 1.98.0 (требование
  1.85+ выполняется), node 26.13.1, pnpm 12.3.4 (глобальный; pin
  `packageManager: pnpm@11.24.0` не мешал — `pnpm install --frozen-lockfile`
  и все скрипты прошли без fallback), sing-box 1.14.4 (fetch-core, sha256
  по `core-version.txt`), 28 ядер / 62 GiB RAM (полная сборка workspace
  с 393 крейтами — ~40 с).
- GNOME на Wayland (gnome-shell), docker с 7 бриджами (172.17–172.24/16),
  сеть wlan0, шлюз 192.168.0.1, systemd-resolved.
- SSH из LAN: kill switch переживает сессию — предсказание runbook §0
  (`ct state established` нет, но LAN-daddr разрешён) подтверждено живьём.
- Поставлены `socat`, `patchelf` (pacman); `jq` был.

### Что проверено (runbook §2–§7)

- **§2:** `cargo test --workspace --locked` — 427 passed / 1 ignored /
  0 failed (на macOS 444; разница — `cfg(target_os)`, провалов нет);
  typecheck/lint/vitest — 114 passed (15 файлов). sing-box 1.14.4
  linux_amd64 верифицирован SHA256.
- **§3:** `SPLITROUTE_ALLOW_UNSIGNED=1 ./scripts/package.sh` — .deb 34.39 МиБ,
  .rpm 34.39 МиБ, AppImage 137.76 МиБ (совпадает с §17).
- **§4:** headless-smoke helper'а под root — 10/10 PASS: сокет 0600,
  status→start→runtime (config.json 0600, sing-box.log)→порт→outbound
  HTTP 200 через mixed→stop→restart→сокет удалён. Формат протокола 2:
  аргументы команды в `payload` (`{"command":"start","payload":{"config":…,
  "kill_switch":false}}`) — пример в runbook §4 был устаревшим, исправлен.
- **§5:** TUN+per-app: интерфейс поднимается (имя `tun0` — генератор не
  задаёт `interface_name`), ip-правила 9000–9010, таблица 2022; per-app
  доказан (curl из TUN → egress через прокси ≠ домашний; wget → домашний);
  DNS-hijack работает: запросы systemd-resolved уходят в vless DoT
  1.1.1.1:853; локалка доступна (ip_is_private → direct).
- **§6/§6b:** kill switch nftables: таблица `splitroute_killswitch`, правила
  как в `rules.rs` (lo, LAN v4/v6, skuid 0, drop); egress пользователя
  блокируется, root проходит; **fail-closed при смерти ядра** (kill по PID
  из `status`): правила остаются, egress остаётся заблокированным; `stop`
  снимает таблицу и восстанавливает egress; killswitch.nft появляется/
  удаляется в runtime.
- **§7 (GUI):** окно рендерится не белым (`WEBKIT_DISABLE_DMABUF_RENDERER=1`);
  подключение/отключение через dev-helper (root, `--allow-uid 1000`);
  **per-app из GUI**: Telegram через VPN подтверждён пользователем
  («без него не работает»), удалённый из правил браузер — домашний IP;
  статистика (Clash API) работает; каталог приложений сканируется,
  добавление/удаление правил ок; «Проверить задержки» работает (~15 с —
  медленно, UX-заметка); установка helper'а из приложения (pkexec) → unit
  `splitroute-helper.service` enabled+active, бинари в `/usr/lib/splitroute/`,
  `RuntimeDirectoryMode=0711`, `--allow-uid 1000`, системный сокет
  `/run/splitroute/helper.sock` достижим процессом пользователя (probe
  status отвечает протоколом 2); конфиг, сгенерированный приложением на
  живом подключении, содержит `gvisor` — фикс подтверждён end-to-end.

### Находки и баги

- **[fix `3810437`] `stack: mixed` ломает TCP через TUN на Linux в
  окружении с docker.** system-стек TCP sing-box (в `mixed` TCP идёт через
  него) молча терял ВСЕ TCP-потоки из TUN: процесс опознаётся, но outbound
  dial не выполняется; UDP (gvisor) и внутренний DNS (vless TCP к
  1.1.1.1:853) работали → «интернет пропал на всей машине при живой
  локалке/SSH». Диагностика по логам ядра (286 строк: 18 vless-dial — все
  к 1.1.1.1:853, 9 UDP packet, 0 TCP-dial из TUN). `stack: gvisor` — TCP
  работает. Фикс: `tun_stack()` в `config/mod.rs` (Linux → gvisor, macOS →
  mixed), тест `tun_stack_is_gvisor_on_linux_and_mixed_elsewhere` (зелёный
  на обеих ОС, 444/427). Предположительная причина отказа system-стека —
  nftables-redirect конфликтует с docker-правилами (7 бриджей); не
  вскрывалось глубже — gvisor достаточно.
- **comm обрезан: `pkill -x sing-box` не матчится.** Dev-ядро
  `sing-box-x86_64-unknown-linux-gnu` → comm `sing-box-x86_64` (15 симв).
  Из-за этого «PASS: sing-box убит» в первом прогоне §6 был ложным
  (не-матчящийся pkill + симметричный pgrep). §6b перепроверил: PID из
  `status` → kill → правила живы, egress заблокирован, stop снимает.
  Правило: ядро убивать по PID из ответа helper'а.
- **Зомби-окно после kill.** `ps -p <pid>` ещё ~секунды показывает процесс
  (зомби: helper не сделал `wait()`), но `pgrep -f sing-box` уже не
  матчится (cmdline пуст). Факт смерти ядра — по pgrep -f, не ps.
- **Пересечение TUN-подсети с docker.** TUN адрес 172.19.0.1/30
  (`TUN_ADDRESS_V4`) пересекается с docker-бриджем 172.19.0.1/16 на этой
  машине. На gvisor ядро работало корректно; задокументировано как
  известный риск для машин с docker-сетями 172.19.x.
- **Exit-IP ≠ entry-IP.** Egress через прокси не обязан совпадать с
  адресом входа сервера (у провайдера edge≠exit). Проверка per-app —
  сравнением curl(TUN) vs wget(direct) (или vs mixed-inbound), а не с
  адресом сервера из подписки.
- **ALREADY_RUNNING.** Повторный `start` без `stop` →
  `ALREADY_RUNNING («ядро уже запущено»)`, а не молчаливое пересоздание
  (ожидание §6.5 runbook было неверным, исправлено). `KillSwitchFailed`
  при этом не возвращается — как и требовалось.
- **Правила per-app применяются только при переподключении** — конфиг
  генерируется на `start`; hot-reload правил не реализован (дизайн,
  UX-заметка; если станет нужен — отдельная задача).

### Грабли для будущих сессий

- **sudo: `$HOME=/root`** — скрипты под sudo не должны строить пути
  пользователя из `$HOME`; брать `getent passwd <user>`.
- **`sudo a; b; c`** — sudo применяется только к первому слову цепочки.
- **pgrep -f само-матчится** через ssh (паттерн в cmdline самого шелла) —
  приём `pgrep -f "splitroute[-]helper"`.
- **pkill -f по подстроке задевает похожие процессы**: паттерн
  `target/debug/splitroute` матчится и в `splitroute-helper`; убивать по
  PID или точным путям.
- **single-instance**: запущенный AppImage/установленное приложение
  держит DBus-lock — dev-копия умирает мгновенно (tauri CLI отдаёт
  SIGTERM → exit 143, «виснет» без окна). Окно AppImage при этом
  показывает собственный `HELPER_NOT_INSTALLED` (его проверка —
  `/run/splitroute/helper.sock`).
- **HELPER_NOT_INSTALLED в dev** = мёртвый env-сокет. Проверять, что env
  долетел: `tr '\0' '\n' </proc/<pid>/environ | grep SPLITROUTE`.
- **Плашка «Не установлен» после успешной установки helper'а** в
  dev-потоке — бейдж отражает доступность env-сокета (dev-helper был
  мёртв), а не факт установки системного. В production-потоке сокет один.
- **Трей на GNOME** без AppIndicator-расширения не отображается; закрытие
  окна завершает приложение (нет hide-to-tray fallback).
- **wget `-T 15` без `--tries=1` ретраится ~20 раз** (минуты зависания);
  у curl нет опции `--tries` (это wget) — у curl `-m`/`--retry`.
- **`nft list` форматирует множества многострочно** — grep по точной
  строке правила ложно падает; проверять по ключевым элементам
  (`10.0.0.0/8`, `fe80::/10`).
- **«NOERROR» в DNS-логах** ложно матчится grep'ом `ERROR`.

### Осталось

- DNS `.local`/mDNS через туннель не проверен (Avahi не разворачивался).
- Сон/смена сети — не проверены (вне runbook этой сессии).
- Windows-часть S4-T5c (WFP-отчёт) — остаётся.
- На CachyOS оставлены: установленный системный helper (решение
  пользователя) и дерево `~/Dev/splitroute` (13 МБ код + 5.9 ГБ target).
  Временные артефакты (`/tmp/sr-linux` с state.json реальной подписки)
  удалены в §8.

## 35. Windows WFP-отчёт — S4-T5c закрыт (проверено 2026-11-22)

Первая живая проверка WFP kill switch'а на реальной Windows 11 (10.0.26100),
полностью под управлением агента: скрипты проверки и отката в
`scripts/windows/`, среда собрана на пустой машине (rustup, pnpm, MSVC Build
Tools уже стояли). Найдены и исправлены **два реальных бага helper'а**,
оба не ловились юнит-тестами (план фильтров тестировался, реальные вызовы
WFP — нет). Коммиты фиксов см. TASKS S4-T5c.

### Как проверялось

- Скрипты `scripts/windows/wfp-report.ps1` (основной) +
  `wfp-watchdog.ps1` (аварийный откат). Основной самодостаточен: baseline
  (`netsh wfp show filters` в XML), dev-helper `console` на пайпе
  `splitroute-dev` (`--allow-sid <SID>`), `start` с `kill_switch:true`,
  пробы, kill ядра, fail-closed, `stop`, верификация; отчёт пишется на диск
  по ходу. Запуск — elevated PowerShell (UAC), `Start-Process -Verb RunAs`.
- Минимальный конфиг ядра без TUN: только `mixed` inbound на
  `127.0.0.1:18999` — для WFP-отчёта туннель не нужен, достаточно живого
  процесса ядра (helper health-check — только «процесс жив»).
- Сторож: отдельный скрытый elevated-процесс с дедлайном 240 с; по дедлайну —
  `stop` по пайпу, kill dev-helper по PID-файлу, `Restart-Service BFE`
  (не персистентные объекты умирают с BFE; mpssvc поднимается следом).
  В нормальном прогоне выходит чисто по done-файлу.
- Отчётная машина: установленное приложение `C:\Program Files\SplitRoute\`
  и системный helper-служба `splitroute-helper-service` (Running, auto) —
  **не тронуты**; dev-helper работал параллельно на отдельном пайпе.
- Пробы при включённом kill switch: loopback OK, LAN (шлюз) OK,
  `1.1.1.1:443` **refused** (не timeout!), трафик через ядро
  (`curl --proxy 127.0.0.1:18999 https://www.gstatic.com/generate_204`) → 204.

### Найдено и исправлено

1. **Перечисление фильтров WFP с `layerKey = GUID_NULL` всегда падает —
   `FWP_E_LAYER_NOT_FOUND` (0x80320004), даже когда совпадающие фильтры
   существуют.** Первый прогон: `KILL_SWITCH_FAILED` на чистой машине.
   Изначальная гипотеза «пустой матч → терпимый код» была опасной: если бы
   шаблон был битым, `stop` никогда не удалял бы фильтры (машина осталась бы
   заблокированной). Изолированная лабораторная проба (`%TEMP%`, не в репо)
   перебрала варианты шаблона: **конкретный слой** (`ALE_AUTH_CONNECT_V4`/`_V6`)
   + `providerKey` + `FULLY_CONTAINED` + `actionMask=0xFFFFFFFF` работает,
   пустой результат на валидном слое приходит как ОК с нулём фильтров;
   `GUID_NULL` в layerKey падает всегда (с фильтрами и без). Фикс
   `delete_objects`: перечисление по обоим слоям по очереди. По SDK-заголовку
   (`winerror.h` 10.0.26100) 0x80320004 = `FWP_E_LAYER_NOT_FOUND`;
   `FWP_E_FILTER_MATCH_NOT_FOUND` в этом SDK не существует.
2. **Гонка named pipe: ответ терялся («пустой ответ» у клиента).** Сервер
   пишет ответ и сразу `DisconnectNamedPipe`, который выбрасывает
   незачитанные клиентом данные из буфера — клиент видит EOF. Мгновенные
   ответы (`status`, `ALREADY_RUNNING`) проигрывали гонку случайно —
   до половины таких вызовов. Фикс `pipe.rs`: `FlushFileBuffers` перед
   disconnect'ом (доктор MSDN для этого случая) и в SID-ветке отказа.
3. **Latent-линт в Windows-only коде каталога**
   (`catalog/windows.rs` needless_borrow): никогда не проверялся clippy —
   до этого дня кросс-проверки были только `cargo check/clippy
   --target x86_64-pc-windows-msvc` на macOS/Linux, которые этот код не
   компилировали (feature-unification). Нативный `cargo clippy --workspace`
   на Windows — обязательная часть проверок отсюда.

### Подтверждено в рантайме (финальный прогон — ALL PASS, 21/21)

- `start` c `kill_switch:true`: правила встают (18 ссылок на объекты
  SplitRoute в XML-дампе WFP: 17 фильтров + подслой + провайдер), ядро
  стартует, `ALREADY_RUNNING` при повторном `start`.
- При активном kill switch: loopback и LAN живы, обычные приложения
  блокируются (TCP `1.1.1.1:443` — refused), трафик через ядро работает
  (HTTPS через `mixed`-инбаунд → 204) — дизайн «разрешить только ядро и
  локальные сети» подтверждён.
- Kill ядра по PID: `state: stopped`, `kill_switch.enabled: true`, правила
  остаются, egress по-прежнему refused — fail-closed.
- `stop`: kill_switch off, все объекты WFP удалены (0), интернет возвращён.
- Два фикса подтверждены повторными прогонами: перечисление — первый
  фикс (прогон с `KILL_SWITCH_FAILED` → прогон с включённым kill switch),
  FlushFileBuffers — «пустой ответ» исчез полностью.

### Грабли и ограничения

- **WFP-блок на Windows = мгновенный refused, а не таймаут** (pf на macOS
  молча дропает). Пробам блокировки не хватает условия «не open»:
  `refused` от внешнего адреса возможен только от локального фильтра.
- **UAC**: скрипты запускаются `Start-Process -Verb RunAs -Wait` — каждый
  прогон требует клика; фоновые elevated-процессы наследуют elevation.
- **Vite dev слушает `[::1]:1420`** (IPv6) — grep по netstat должен
  учитывать обе семьи.
- **`Start-Process -RedirectStandardOutput/Error`** обязательные параметры
  передавать явно — объявленные переменные без передачи ничего не пишут
  (потерянные логи первой пробы).
- **UTF-16 вывод PowerShell `Out-File`** — читать через
  `Get-Content`/`iconv`; `cat` в Git Bash показывает mojibake.
- `taskkill //F //PID` в Git Bash — двойные слэши (MSYS конвертит `/PID`).
- `corepack` в Node 26 нет — pnpm ставить `npm i -g pnpm`.
- Статистика: `netsh wfp show filters file=<path>` пишет XML ~МБ; счётчик
  объектов — grep по GUID провайдера; baseline до и после = 0.
- Регресс-тест фикса перечисления: `crates/helper/tests/wfp_live.rs`
  (`#[ignore]`, нужен elevation:
  `cargo test -p splitroute-helper --test wfp_live -- --ignored`).
- Осталось: kill switch на Windows с реальным профилем и GUI (проверено
  только через протокол helper'а), TUN-трафик на Windows (в отчёте —
  loopback-инбаунд вместо туннеля), wintun-драйвер в бандле — при
  `tauri build`.

### 36. Windows: весь функционал через GUI под Computer Use (проверено 2026-11-22)

Вторая половина дня той же сессии: все GUI-функции прогнаны агентом через
Computer Use (a11y-дерево WebView2 + скриншоты), пользователь вводил только
секретный URL подписки и жал UAC. Dev-стек: helper на пайпе `splitroute-dev`
(elevated), приложение с `SPLITROUTE_HELPER_PIPE`+`SPLITROUTE_STATE`
(изолированный стор). Стек тестов `scripts/windows/` из §35 дополнен
GUI-сценариями; fail-closed с GUI — временным скриптом (см. грабли).

#### Результаты

- **Подключение/ядро**: TUN-адаптер `tun0` (sing-tun) поднят, wintun 0.14.1
  работает в dev; статистика сессии (↑12,4 МиБ ↓372,9 КиБ), таймер.
- **Per-app**: правило по процессу (`powershell.exe` «Через прокси: Да») +
  переподключение → exit-IP powershell = VPN (203.57.82.185), curl без
  правила = домашний (95.105.52.2). Mixed-inbound наследует правило
  процесса-клиента — через него direct-клиент тоже идёт мимо VPN (это
  ожидаемо: правило матчится по процессу-владельцу сокета).
- **Kill switch через GUI**: настройка «Блокировать прямой трафик» ON →
  переподключение → 18 объектов WFP (35 ссылок в elevated-дампе XML).
  Смерть ядра (elevated kill): direct-трафик **BLOCKED** на T+0.5
  (fail-closed), автореконнект поднял ядро с новым PID за ~2 с — окно
  блокировки 2–3 с. Отключение: 0 объектов WFP, сеть возвращена.
- **Автореконнект (S4-T15)**: восстановление с новым PID; подменный бинарь
  (cmd.exe как sing-box.exe — check/run падают мгновенно) → 3 попытки →
  исчерпание; UI: красный бейдж «Ошибка», карточка «Ядро остановлено.
  Kill switch блокирует прямой трафик — отключитесь, чтобы снять защиту»,
  kill switch «Активен», кнопка «Отключить и снять защиту»; ручное
  «Подключить» после возврата бинаря — сессия поднялась (сброс исчерпания).
- **QR-импорт**: файл → декод rqrr → форма с префиллом; Ctrl+V вставкой
  изображения из буфера — **работает в WebView2** (для §33: mime —
  bitmap,paste проходит); негативы: vless-QR → `QR_NODE_LINKS`
  («Импорт узлов без подписки пока не поддерживается»), не-QR →
  `QR_DECODE_FAILED`, «Отмена» ничего не открывает. Drag&drop не
  автоматизирован (CUA не тянет OS-перетаскивание файлов) — за
  пользователем.

#### Найдено и исправлено

1. **`subscription_add`/`subscription_update` падали с «Код: UNKNOWN»**:
   Tauri 2 ждёт camelCase аргументы (`userAgent`), обёртки шлют snake_case
   (`user_agent`) — десериализация аргументов не сходилась, invoke
   отклонялся объектом без кода. Фикс: `#[tauri::command(rename_all =
   "snake_case")]` на обеих командах. `qr_decode_image` не задет (там JS
   случайно шлёт `dataBase64`). Регресс этапа 6: юнит-тесты с мокнутым api
   не видят шейп аргументов; живой GUI-прогон — видит. Коммит `1c8ae0e`.
2. **Git Bash/MYSYS искажает env-значения с `\.\pipe\...`** — приложение
   получало битый путь пайпа (os error 3, «Не установлен» при живом
   helper'е). Запускать dev-приложение через cmd/PowerShell-обёртку, env —
   внутри неё (`sr-gui-launch.cmd`). На Linux/macOS этой грабли нет.
3. **PowerShell `Set-Content -Encoding UTF8` пишет BOM** — правка
   state.json из PS ломает парсинг в Rust (serde_json не читает BOM), стор
   молча откатывается к пустому. Писать через
   `[IO.File]::WriteAllText($path, $text, [Text.UTF8Encoding]::new($false))`.
4. **taskkill без elevation не убивает ядро**, запущенное elevated-helper'ом
   («Отказано в доступе»); fail-closed тест — только из elevated-обёртки.

#### Грабли и наблюдения

- **Задержки при активном подключении показывают ~1 мс** на обе локации
  (замер идёт с подключённым туннелем) — до подключения значения будут
  честные; при деградации-алертах учитывать.
- PS 5.1: строка `"$tag: ..."` парсится как drive-ссылка — писать
  `"${tag}:"`; .ps1 без BOM читается как ANSI — в скриптах только ASCII.
- `netsh wfp show filters file=` требует elevation — из не-admin-сессии
  XML не пишется молча.
- Экран «Все правила»: вкладки не в a11y-дереве (стилизованные div) —
  клик по координатам растра; правило из каталога создаётся ВЫКЛЮЧЕННЫМ —
  тумблер «Через прокси» на вкладке правил.
- HWID свежего стора серверу неизвестен → подписка отдаёт 0 профилей
  (NO_PROFILES): для dev-проверок копировать HWID из стора установленного
  приложения (привязка HWID работает и на Windows).
- Дубли кнопок a11y (элемент и текст) — индексы перепроверять свежим
  деревом после каждого действия.
