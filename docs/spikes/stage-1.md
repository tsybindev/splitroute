# Отчёт этапа 1 — spike: TUN, process-правила, DNS, helper

Дата: 2026-11-12. Задачи: S1-T1…S1-T12 (`TASKS.md`).

Короткий вывод: **архитектура «sing-box sidecar + TUN + process-правила + root-helper»
не опровергнута и подтверждена там, где её можно было проверить без root.** На
macOS 15.8.2 запуском доказаны process-правила для TCP и UDP, матчинг
helper-процессов браузера по `.app` и разделение DNS по процессу. TUN целиком,
macOS 26/27 (риск R1), Linux и Windows остались непроверенными по причинам
окружения; это зафиксировано как «не проверено», а не как «работает».

## 1. Окружение

| Параметр | Значение |
|---|---|
| ОС | macOS 15.8.2 (24H232), arm64 |
| Ядро | sing-box 1.14.4 (pinned, `scripts/core-version.txt`), сборка go1.26.10 darwin/arm64, revision `c005f35` |
| Python | 3.14.7 |
| Go | 1.27.0 (для сборки `spike/macos/socksprobe.go`) |
| Rust / cargo | отсутствует |
| Права | без root: `sudo -n true` → «a password is required» |
| Подписка | `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID` не заданы; реального сервера нет |
| Сеть | активен сторонний VPN (v2RayTun), интерфейсы `utun0…utun4` |
| Chrome | 157.0.8103.51 |

## 2. Методика

Общий принцип: не выдавать непроверенное за проверенное; каждый результат —
воспроизводимая команда и наблюдаемый признак. TUN требует root, поэтому
process-правила проверялись через mixed-inbound: клиент подключается к SOCKS5-порту
sing-box, а route-правило решает, в какой outbound уйдёт трафик. Наблюдаемость —
через разные маркеры ответа, а не «на глаз».

Подробные методики и логи — в `spike/macos/README.md` и `spike/helper/README.md`.

## 3. Результаты

### 3.1. Конвертация подписки (S1-T1…T3, ранее)

- pinned sing-box 1.14.4 с проверкой SHA256 (`scripts/fetch-core.sh --verify`);
- санитизированная фикстура Happ-JSON: 2 профиля, служебные outbound'ы;
- `spike/convert.py`: VLESS + Reality + TCP → outbound sing-box;
- оба конфига проходят `sing-box check`; 10 тестов конвертера зелёные.

### 3.2. process-правила, TCP (S1-T4)

`python3 spike/macos/process_rules_test.py` — 5/5 кейсов:

| Кейс | Факт |
|---|---|
| `process_path_regex` — разрешённый путь | `MARKER-PROXY` ✅ |
| `process_path_regex` — другой путь | `MARKER-DIRECT` ✅ |
| `process_name` — совпадающее имя | `MARKER-PROXY` ✅ |
| `process_name` — другое имя | `MARKER-DIRECT` ✅ |
| Chrome (headless) через `^/Applications/Google Chrome\.app/` | `MARKER-PROXY` ✅ |

Наблюдения:

- **R3 подтверждён и митигируется.** Helper-процессы Chrome
  (`…Helpers/Google Chrome Helper (Aperitif).app/…`) имеют другие имена, но тот же
  каталог `.app`; `process_path_regex` их матчит. Это ровно та митигация, что
  записана в PRD §6.3.
- **R2 подтверждён.** Дочерний процесс (`curl`/`socksprobe` из оболочки) матчится
  по собственному пути/имени, а не по родителю. Для CLI нужны явные правила на
  бинарь или каталог.
- Два практических нюанса для этапа 2: пути на macOS канонические
  (`/private/var/...`, а не `/var/...`); `override_address` у direct-outbound
  удалён в sing-box 1.13+ — использовать route-правила или отдельные outbound'ы.

### 3.3. process-правила, UDP (S1-T9)

`python3 spike/macos/udp_rules_test.py` — 2/2: выбранный процесс → `UDP-PROXY`,
другой → `UDP-DIRECT`. UDP проходит через ядро, process-правила к UDP применяются.

Ограничение: QUIC поверх TUN и реального VLESS-сервера не проверялся.
Предварительное решение: **не блокировать QUIC выбранных приложений по умолчанию**,
но иметь переключатель «блокировать QUIC» на случай деградации. Окончательное
решение — после TUN-теста на реальном сервере.

### 3.4. DNS (S1-T8)

`python3 spike/macos/dns_rules_test.py` — 2/2: выбранный процесс резолвится через
`remote`, невыбранный — через `local` (по факту прихода запросов на два разных
DNS-сервера).

Ограничение: перехват DNS через TUN, поведение с `systemd-resolved` и Private Relay
не проверялись — нужны root, Linux и macOS 26. По PRD §6.4 при провале разделения —
fallback на системный DNS с предупреждением в UI, без молчаливых утечек.

### 3.5. helper (S1-T10)

`python3 -m unittest spike/helper/test_helper.py -v` — 6/6: сокет 0600, uid пира,
`status`/`start`/`stop`, реальный запуск sing-box, неизвестная команда,
несовпадение версии протокола, невалидный JSON, лимит размера. После тестов
зависших ядер нет.

Прототип на Python, потому что Rust-тулчейна нет. Черновики LaunchDaemon и
systemd лежат в `spike/helper/deploy/` и **не устанавливались**. Продакшн-helper —
Rust, этап 2–3.

### 3.6. TUN

TUN не поднимается без root:

```text
FATAL start service: start inbound/tun[tun-in]: configure tun interface: Connect: operation not permitted
```

Это не результат «TUN не работает», а ограничение окружения. Сравнение внешнего IP
выбранного и невыбранного процесса не выполнялось: нет реальной подписки.

## 4. Go/no-go по платформам

| Платформа | Статус | Обоснование |
|---|---|---|
| macOS 15.x | **go (механика)** | process-правила TCP/UDP и DNS по процессу подтверждены; TUN целиком не проверен (нет root/сервера) |
| macOS 26/27 | **нет данных** | машины нет; R1 не воспроизведён и не опровергнут. Считать риском, держать план Б (NetworkExtension) |
| Linux (CachyOS) | **нет данных** | машины нет; поддержка заявлена документацией sing-box, но не проверена |
| Windows | **нет данных** | VM нет; `wintun.dll`, права, UDP — не проверены |

Итог: **условный go** для macOS 15 на этап 2; остальные платформы нельзя объявлять
поддержанными до проверки. Блокеров в подходе process-правил не найдено.

## 5. Статус рисков PRD §10 после этапа 1

| # | Риск | Статус на 2026-11-12 |
|---|---|---|
| R1 | process-правила на macOS 26/27 | **не проверено** (нет ОС); план Б — NetworkExtension — остаётся |
| R2 | матчинг дочерних процессов | **подтверждён**; митигация: правила на бинарь/каталог, объяснение в UI |
| R3 | helper-процессы браузеров | **подтверждён**; митигация `process_path_regex` по каталогу `.app` работает |
| R4 | UDP/QUIC | UDP + process-правила подтверждены; QUIC поверх TUN — нет данных; переключатель блокировки QUIC |
| R5 | конфликт с другим VPN/TUN | на dev-машине одновременно активны сторонний VPN и `utun0…utun4`; детект конфликта нужен, но не реализован |
| R6 | DNS-утечки, `systemd-resolved`, Private Relay | разделение DNS по процессу подтверждено; утечки через TUN и указанные механизмы не проверены |
| R13 | fail-open при падении ядра | не проверялся; поведение и автореконнект — этап 2 |

## 6. Что осталось проверить (перенесено)

1. TUN целиком на macOS 15 с правами root и реальным сервером: маршрутизация,
   сравнение внешнего IP, очистка TUN/маршрутов.
2. macOS 26/27: воспроизведение R1 (нужна машина/CI).
3. CachyOS: пакеты, права, TUN, process-правила, `systemd-resolved`.
4. Windows: TUN, `wintun.dll`, служба под SYSTEM, process-правила, UDP.
5. DNS-утечки через TUN и Private Relay; QUIC поверх TUN.
6. Установка LaunchDaemon/systemd, подпись helper'а — этап 3.

## 7. Артефакты

| Путь | Содержимое |
|---|---|
| `spike/convert.py`, `spike/test_convert.py` | конвертер и 10 тестов |
| `spike/configs/` | конфиги, прошедшие `sing-box check` |
| `spike/macos/process_rules_test.py` | TCP + Chrome |
| `spike/macos/udp_rules_test.py` | UDP + process-правила |
| `spike/macos/dns_rules_test.py` | DNS по процессу |
| `spike/macos/socksprobe.go` | SOCKS5-клиент (TCP/UDP) |
| `spike/helper/` | прототип helper'а, 6 тестов, черновики сервисов |

Все значения в фикстурах синтетические; реальные URL, HWID, UUID и адреса в
репозиторий не попадали.
