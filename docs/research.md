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
| Windows (WFP/firewall) | **не реализовано** | включение возвращает `KILL_SWITCH_FAILED`; `S4-T5b` |
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
- Протокол поднят до версии 2, потому что helper v1 молча проигнорировал бы
  `kill_switch` и клиент считал бы трафик защищённым. Несовместимость
  обнаруживается на `ProtocolMismatch`, UI предлагает переустановку.
- Сон/пробуждение и смена сети: правила не содержат адресов сервера и не зависят
  от интерфейса; DHCP-клиент работает под root и разрешён. Отдельного
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
