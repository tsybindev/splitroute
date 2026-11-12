# S1-T4…T7: TUN и process-правила

Отчёт по подцели G1. Что проверено запуском, что — нет и почему.

## Окружение

| Параметр | Значение |
|---|---|
| ОС | macOS 15.8.2 (24H232), arm64 |
| Ядро | sing-box 1.14.4 (pinned, `scripts/core-version.txt`), собран go1.26.10 darwin/arm64 |
| Python | 3.14.7 (только stdlib) |
| Go | 1.27.0 (нужен для сборки `socksprobe.go`) |
| Права | без root: `sudo -n true` → «a password is required» |
| Сеть | на машине активен сторонний VPN (v2RayTun) и интерфейсы `utun0…utun4` |
| Подписка | `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID` в сессии не заданы, реального сервера нет |

## Методика (зафиксирована до запуска)

TUN требует root, а root в сессии недоступен. Но process-правила sing-box резолвятся
по локальному сокету-источнику, поэтому проверяем их без TUN:

1. sing-box поднимается **без TUN** с mixed-inbound на `127.0.0.1:<свободный порт>`.
2. Два outbound'а — `socks` на два локальных фейковых SOCKS5-сервера. Любой `CONNECT`
   тот отвечает `HTTP 200` с разным телом:
   - outbound `proxy` → `MARKER-PROXY`;
   - outbound `direct` (он же `final`) → `MARKER-DIRECT`.
3. Тестируемый клиент (`socksprobe`) идёт через SOCKS5 mixed-inbound на публичный
   адрес TEST-NET-1 `192.0.2.10`. Реального соединения в интернет нет.
4. Смотрим, какой маркер вернулся: он однозначно говорит, какое route-правило
   сработало.

Почему `socksprobe`, а не `curl`: `/usr/bin/curl` — restricted platform binary,
его копии в `$TMPDIR` убиваются AMFI (`SIGKILL`, код −9). Свой Go-бинарь можно
копировать под разными именами и путями — это и нужно для `process_name` и
`process_path_regex`.

`override_address`/`override_port` у `direct`-outbound не используются: они удалены
в sing-box 1.13+. Поэтому наблюдаемость сделана через `socks`-outbound.

## Результат: process-правила работают

`python3 spike/macos/process_rules_test.py` — код возврата 0.

| Кейс | Действие | Ожидание | Факт |
|---|---|---|---|
| `process_path_regex` | `…/path-a/socksprobe` | `MARKER-PROXY` | `MARKER-PROXY` ✅ |
| `process_path_regex` | `…/path-b/socksprobe` (путь вне regex) | `MARKER-DIRECT` | `MARKER-DIRECT` ✅ |
| `process_name` | `colocated/socksprobe` | `MARKER-PROXY` | `MARKER-PROXY` ✅ |
| `process_name` | `colocated/socksprobe-b` (имя другое) | `MARKER-DIRECT` | `MARKER-DIRECT` ✅ |
| `process_path_regex` Chrome | headless Chrome, regex `^/Applications/Google Chrome\.app/` | `MARKER-PROXY` | `MARKER-PROXY` ✅ |

Ключевое наблюдение из лога sing-box (Chrome, R3):

```text
router: found process path: /Applications/Google Chrome.app/Contents/MacOS/Google Chrome, user: …
router: found process path: /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/…/Helpers/Google Chrome Helper (Aperitif).app/Contents/MacOS/Google Chrome Helper (Aperitif), user: …
outbound/socks[proxy]: outbound connection to clients2.google.com:443
```

То есть helper-процессы браузера имеют **другие имена**, но тот же каталог `.app`
и матчатся `process_path_regex` — это подтверждает риск R3 и выбранную митигацию
(правило по каталогу `.app`, а не только по имени).

Второе наблюдение (R2): `curl`/`socksprobe`, запущенный как дочерний процесс,
матчится по **собственному** пути и имени, а не по родителю (Python/оболочке).
Значит правило «пустить `git`/`curl` через прокси» требует явных правил на каждый
бинарь или на каталог — как и записано в PRD §6.3.

### Chrome: нюанс воспроизведения

Headless Chrome через SOCKS5-proxy sing-box падает на этапе проверки прокси
(`net::ERR_PROXY_CONNECTION_FAILED`, sing-box видит «peek first byte: EOF»), а
через HTTP-proxy mixed-inbound работает. Поэтому в стенде Chrome запускается с
`--proxy-server=http://127.0.0.1:<port>`. Это особенность headless-Chrome, а не
process-правил: в продуктовой схеме Chrome ходит через TUN, без явного прокси.

## Что НЕ проверено и почему

| Задача | Статус | Причина |
|---|---|---|
| S1-T4, сам TUN (`auto_route`, `strict_route`, трафик через TUN) | **не проверено** | нет root: `sudo -n` требует пароль. Запуск конфига с TUN под пользователем даёт `start inbound/tun[tun-in]: configure tun interface: Connect: operation not permitted` (код 1) |
| S1-T4, сравнение внешнего IP выбранного/невыбранного процесса | **не проверено** | нет реальной подписки (`SUBSCRIPTION_URL` не задан) и реального сервера; внешний IP сравнивать не с чем |
| S1-T5, macOS 26/27 и риск R1 | **не проверено** | на машине только macOS 15.8.2; обновление ОС в рамках спайка недопустимо. Требуется отдельная машина/CI |
| S1-T6, CachyOS | **не проверено** | машины с CachyOS нет; Docker-демон OrbStack не запущен (частичный тест в контейнере не делался) |
| S1-T7, Windows VM | **не проверено** | VM/UTM/Parallels/VirtualBox/qemu в системе отсутствуют |

Вывод по G1: **process-правила на macOS 15 подтверждены для TCP**, включая
helper-процессы Chrome. Маршрутизация TUN, DNS/QUIC через TUN и поведение на
macOS 26/27, Linux и Windows остаются непроверенными по указанным причинам.

## Воспроизвести

```bash
cd app
# бинарь ядра
./scripts/fetch-core.sh --verify

# process-правила без root (нужен Go для сборки socksprobe)
python3 spike/macos/process_rules_test.py

# только path_regex/process_name, без Chrome
python3 spike/macos/process_rules_test.py --no-chrome

# доказательство, что TUN без root не поднимается
./src-tauri/binaries/sing-box-aarch64-apple-darwin run \
  -c spike/configs/profile-1-testovaya-lokatsiya-a.json   # operation not permitted
```

## Чистота системы

Тест не требует root и не меняет маршруты/интерфейсы. Скрипт сам:
останавливает sing-box по завершении, закрывает фейковые SOCKS5-серверы, пишет
временные конфиги и логи в `$TMPDIR`, не трогает `spike/configs/`.
Проверить, что ничего не осталось:

```bash
pgrep -fl 'sing-box run' || echo "sing-box не запущен"
```

## Файлы

- `process_rules_test.py` — стенд (stdlib Python; собирает `socksprobe`).
- `socksprobe.go` — SOCKS5-клиент, копируется под разными именами/путями.
