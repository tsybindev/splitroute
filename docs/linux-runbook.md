# Linux-runbook: smoke-проверка рантайма по SSH (S2-T9 / S3-T7)

> Готовится на macOS (2026-11-20), выполняется на Linux-машине (CachyOS) по
> SSH. Закрывает Linux-часть этапов 2–3: всё, что CI не проверяет
> (research §27 — только компиляция/unit-тесты/сборка бандлов).
> Факты в этом документе взяты из кода (`privileged/linux.rs`,
> `killswitch/rules.rs`, `catalog/linux.rs`), `AGENTS.md` §5.1/§траблшутинг,
> `docs/stage-3-report.md` §9 и `docs/research.md` §17/§20.

## 0. Безопасность сессии — прочитать до начала

- **Kill switch глушит исходящий трафик всей машины** (fail-closed): правила
  nftables (`crates/helper/src/killswitch/rules.rs:32`) — таблица
  `inet splitroute_killswitch`, цепочка **output only**, правила: lo,
  IPv4 LAN/мультикаст (`10/8`, `172.16/12`, `192.168/16`, `169.254/16`,
  `224/4`), IPv6 link-local/ULA/multicast, `meta skuid 0` (root), далее
  `drop`.
- **`ct state established` в правилах НЕТ**, и вход (`input`) не
  фильтруется. Следствие для SSH:
  - SSH **из локальной сети** (10/8, 172.16/12, 192.168/16) — сессия
    переживёт kill switch: ответные пакеты идут на адрес из разрешённой
    подсети (`ip daddr accept`).
  - SSH **из интернета** — ответные пакеты пользовательского шелла (uid ≠ 0)
    падают в `drop`: сессия замрёт. Выживет только трафик root-процессов
    (`skuid 0`).
  - **Kill switch-тест делать только при SSH из LAN, из root-шелла
    (`sudo -i` сессия даёт skuid 0 — но пользовательская оболочка всё равно
    отвалится) или с консольного доступа к машине.** При сомнении — не делать
    по SSH, отложить до локального доступа.
- kill switch выключен по умолчанию; все остальные проверки безопасны.
- Секреты (реальная подписка, UUID, сервер) в команды, логи, чек-листы и
  коммиты не попадают; конфиги с реальными узлами — только во временном
  каталоге `/tmp`, удалить после проверки.

## 1. Подготовка машины (CachyOS, одноразово)

```bash
# Зависимости (AGENTS.md §5.1)
sudo pacman -Syu --needed base-devel curl wget file openssl git \
  webkit2gtk-4.1 gtk3 libsoup3 libappindicator-gtk3 librsvg patchelf \
  nodejs npm

# Rust и pnpm
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
corepack enable pnpm   # или npm i -g pnpm
rustc --version        # требуется 1.85+ (rqrr), CI использует текущий stable
```

Ubuntu/Debian вместо pacman (пакеты из CI, `.github/workflows/build.yml:33`):
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev
libssl-dev patchelf file`.

## 2. Код, сборка, unit-тесты (headless)

```bash
git clone https://github.com/tsybindev/splitroute.git   # или git pull в существующем
cd splitroute/app
pnpm install
./scripts/fetch-core.sh            # sing-box pinned (scripts/core-version.txt), → src-tauri/binaries/
./scripts/build-helper-sidecar.sh  # helper → src-tauri/binaries/ (нужен cargo test'у из-за externalBin)
cargo test --workspace --locked
pnpm typecheck && pnpm lint && pnpm test
```

Ожидание: все тесты зелёные (на macOS 443 Rust / 114 frontend; на Linux часть
тестов может отличаться по числу из-за `cfg(unix)` — провал не норма).
Записать: числа тестов, версии (`rustc --version`, `sing-box version`).

## 3. Пакеты (headless)

```bash
SPLITROUTE_ALLOW_UNSIGNED=1 ./scripts/package.sh
ls -la target/release/bundle/{deb,rpm,appimage}/
```

Ожидание: `.deb`, `.rpm`, `.AppImage` (на CachyOS в §17 получались 33–34 и
137 МиБ соответственно; пакеты без подписи — ожидаемо). Записать: размеры,
версии. AppImage на машине без FUSE2: `./SplitRoute*.AppImage --appimage-extract`
(AGENTS.md troubleshooting).

## 4. Headless-smoke helper'а (SSH, без GUI) — аналог stage-3-report §9.4

Dev-бинари уже собраны (`cargo build --workspace` при тестах). Конфиг без TUN
— безопасный старт ядра:

```bash
cargo build --workspace
mkdir -p /tmp/sr-linux/{runtime,state}
cat > /tmp/sr-linux/config-notun.json <<'EOF'
{
  "inbounds": [{"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": 18089}],
  "outbounds": [{"type": "direct", "tag": "direct"}],
  "route": {"final": "direct"}
}
EOF

# dev-helper под root (потребует пароль sudo)
sudo target/debug/splitroute-helper \
  --socket /tmp/sr-linux/helper.sock \
  --runtime-dir /tmp/sr-linux/runtime \
  --sing-box "$PWD/src-tauri/binaries/sing-box-x86_64-unknown-linux-gnu" \
  --allow-uid "$(id -u)" &

# проба протокола: newline-JSON, ОДНО сообщение на соединение
# (stage-3 §9.4: второй запрос в тот же сокет — BrokenPipe/EOF, это норма)
printf '{"protocol":2,"id":"1","command":"status"}\n' | socat - UNIX-CONNECT:/tmp/sr-linux/helper.sock
```

Чек-лист по шагам (фиксировать результат каждого):

1. `ls -l /tmp/sr-linux/helper.sock` — сокет есть, права `0600`.
2. `status` → `{"state":"stopped", ...}` (без ядра).
3. `start` с конфигом: `{"protocol":2,"id":"2","command":"start","config":<json>,"kill_switch":false}`
   (лимит сообщения 1 МиБ) → `running` + `pid`.
4. `ls -l /tmp/sr-linux/runtime/` — `config.json` `0600`, `sing-box.log`.
5. `ss -ltnp | grep 18089` — mixed-inbound слушает `127.0.0.1:18089`.
6. Повторный `status` → тот же `pid`.
7. `curl -x http://127.0.0.1:18089 https://example.invalid -m 5` — outbound
   через ядро отвечает (или понятная ошибка сети — ядро живо, это главное).
8. `stop` → `stopped`; процессы и сокет исчезли
   (`pgrep -x sing-box`; `ss | grep 18089` пусто; SIGTERM убирает сокет).
9. Повторный `start` после `stop` — работает (перезапуск без перезапуска
   helper'а).

## 5. TUN и per-app (headless, требует рабочего профиля)

Профиль: единственный источник правды — реальная подписка пользователя.
Скопировать `state.json` с macOS **вне git/логов** (scp) на машину в
`/tmp/sr-linux/state/state.json`, либо вписать outbound реального профиля в
конфиг-шаблон ниже (плейсхолдеры — не коммитить!):

```jsonc
{
  "inbounds": [{"type": "tun", "tag": "tun-in", "interface_name": "splitroute-tun",
    "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
    "auto_route": true, "strict_route": true, "stack": "mixed", "mtu": 1500},
   {"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": 10808}],
  "outbounds": [{"type": "vless", "tag": "proxy", "server": "<сервер>", "server_port": <порт>,
    "uuid": "<UUID>", "flow": "<xtls-rprx-vision|пусто>", "tls": {...Reality-поля из профиля...}},
   {"type": "direct", "tag": "direct"}],
  "route": {"rules": [{"process_name": ["curl"]}], "final": "direct"}
}
```

(`route.rules` — упрощённый аналог генератора; точные правила смотри в
`src-tauri/src/config/mod.rs`. Адреса TUN фиксированные: `172.19.0.1/30`,
`fdfe:dcba:9876::1/126`.)

Чек-лист:

1. `start` → `running`. `ip addr show` — есть `splitroute-tun` с адресами
   `172.19.0.1/30` и `fdfe:dcba:9876::1/126`; `ip route` — маршруты через TUN.
2. Per-app: `curl -m 10 https://api.ipify.org` — curl в `process_name` → IP
   прокси-сервера; `wget -qO- -T 10 https://api.ipify.org` (не в правиле) —
   домашний IP.
3. DNS: режим `proxy` — `resolvectl status`/фактический DNS выбранного
   процесса идёт через туннель (`systemd-resolved`-конфликт — сам сценарий
   S2-T9, зафиксировать поведение); локальные имена (`.local`) — напрямую.
4. `stop` → `ip addr`/`ip route` чистые, процессов нет, конфиг удалён из
   runtime-каталога.

## 6. Kill switch nftables (только по правилам §0!)

Включение — через `start` c `"kill_switch":true` (helper ставит правила до
запуска ядра):

1. `start` (kill_switch:true) → `running`; `nft list table inet
   splitroute_killswitch` — 7 правил как в `rules.rs:32-40` (не найти таблицу
   до включения — норма).
2. Egress от пользователя: `curl -m 5 https://api.ipify.org` — блок (таймаут).
   От root: `sudo curl -m 10 https://api.ipify.org` — проходит (`skuid 0`).
   SSH-сессия жива (см. §0) — если нет: немедленно `nft delete table inet
   splitroute_killswitch` с консоли/второго канала.
3. Убить ядро: `sudo socat - UNIX-CONNECT:<sock>` с `stop`-командой НЕ
   сработает (это честный stop и правила снимутся) — для fail-closed теста
   убить сам sing-box: `sudo pkill -x sing-box` (хвост имени не мешает: на
   Linux бинарь `sing-box`, comm совпадает). После этого helper
   переподключение не делает (это задача приложения), правила остаются:
   egress от пользователя всё ещё блок, `nft list table` жив.
4. `stop` → правила сняты: `nft list tables | grep splitroute` — пусто,
   egress восстановлен, файл `killswitch.nft` из runtime удалён.
5. Идемпотентность: второй `start` c kill_switch:true подряд — без ошибок
   (таблица пересоздаётся), `KillSwitchFailed` не возвращается.

## 7. GUI-часть (нужен дисплей: пользователь у машины / VNC)

Запуск dev-приложения: `SPLITROUTE_HELPER_SOCKET=/tmp/sr-linux/helper.sock
SPLITROUTE_STATE=/tmp/sr-linux/state/state.json pnpm tauri dev`. Белое окно
WebKit — `WEBKIT_DISABLE_DMABUF_RENDERER=1` (AGENTS.md troubleshooting).
Helper на этот раз — dev без `sudo`-конфликта или из §4.

1. Окно открывается не белым; «Главная», «Локации» рендерятся.
2. Подключение/отключение кнопками; бейдж статистики; задержки
   («Проверить задержки» — TCP-connect).
3. Каталог: вкладка «Установленные» сканирует `/usr/share/applications` +
   `~/.local/share/applications`, иконки из hicolor/pixmaps (§17: 19
   приложений, 18 иконок — ориентир).
4. Трей: иконка видима (нужно расширение AppIndicator в DE; на Wayland
   возможны проблемы — AGENTS.md), меню локализовано.
5. Установка helper'а из приложения (кнопка, pkexec-диалог — GUI):
   `systemctl status splitroute-helper` — active (running), unit в
   `/etc/systemd/system/`, бинари в `/usr/lib/splitroute/`, сокет
   `/run/splitroute/helper.sock` `0600`. Удаление — `disable --now`, файлы
   сняты.
6. Установка пакета: `sudo pacman -U` нет для deb — на CachyOS ставить
   `.rpm` (`rpm -i` с зависимостями `webkit2gtk4.1 gtk3 polkit`) или
   распаковать `.deb` для инспекции содержимого (`usr/share/doc/splitroute/`
   — LICENSE/THIRD_PARTY_NOTICES). Запуск установленного приложения —
   `com.splitroute.app` находит установленный helper
   (`/run/splitroute/helper.sock`).

## 8. Очистка и контроль

```bash
pgrep -x sing-box; pgrep -fl splitroute-helper
lsof -nP -iTCP:1420 -sTCP:LISTEN
nft list tables | grep splitroute
ip addr | grep splitroute
ls -d /tmp/sr-linux   # конфиги с реальными секретами удалить!
```

## 9. Фиксация результатов

- Записывать только наблюдаемое; не выдавать непроверенное за проверенное
  (формат — как stage-3-report §9/S3-T7).
- TASKS.md: S2-T9 и S3-T7 — Linux-колонку закрыть фактами (или уточнить
  blocked-остатки), TASKS этапа 8 живёт отдельно.
- `docs/research.md` — новый §34: среда (дистрибутив, ядра, версии), что
  проверено headless/GUI, грабли (как минимум §0-факты про `ct state` и
  SSH, Wayland/AppIndicator, systemd-resolved).
- `NEXT_SESSION.md` — перезаписать.
