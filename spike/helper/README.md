# S1-T10: прототип helper'а

Минимальный helper, который принимает только `start`, `stop`, `status`, проверяет
конфиг ядра и запускает sing-box. Это **спайк**: продакшн-helper будет на Rust
(AGENTS.md §2–§3), а здесь Python, потому что в окружении спайка нет Rust-тулчейна.

## Что проверено

`python3 -m unittest spike/helper/test_helper.py -v` — 6 тестов, все зелёные:

| Проверка | Результат |
|---|---|
| Сокет имеет права `0600` | ok |
| `status` до запуска → `stopped` | ok |
| `start` с валидным конфигом → `running`, реальный pid, mixed-inbound слушает | ok |
| `status` → тот же pid и `running` | ok |
| `stop` → `stopped`, процесс ядра завершён (`os.kill(pid, 0)` → ProcessLookupError) | ok |
| Неизвестная команда (`exec`) → `UNKNOWN_COMMAND` | ok |
| Несовпадение версии протокола → `PROTOCOL_MISMATCH` | ok |
| Невалидный JSON → `BAD_REQUEST` | ok |
| Сообщение > 1 МиБ → `TOO_LARGE` | ok |

После тестов `pgrep -fl 'sing-box run'` пуст — зависших ядер нет.

## Безопасность в прототипе

- Unix-сокет `AF_UNIX`, права `0600`, владелец — текущий пользователь.
- uid пира проверяется: `getpeereid` на macOS, `SO_PEERCRED` на Linux.
- Набор команд фиксирован: `start`, `stop`, `status`. Никаких `exec`, произвольных
  путей, shell и т.п.
- Helper не принимает путь к конфигу: принимает JSON-объект конфига, сам пишет его
  в runtime-каталог с правами `0600` и удаляет при остановке.
- Перед запуском выполняется `sing-box check`; невалидный конфиг не запускается.
- Лимит сообщения 1 МиБ, таймаут чтения 10 с.
- Версия протокола проверяется.
- Содержимое конфига (UUID, ключи) не логируется.

## Протокол

Один JSON на строку, ответ — одной строкой:

```json
{"protocol":1,"id":"uuid","command":"start","payload":{"config":{}}}
{"protocol":1,"id":"uuid","ok":true,"payload":{"state":"running","pid":1234,"version":"sing-box version 1.14.4","started_at":1762950000.0}}
{"protocol":1,"id":"uuid","ok":false,"error":{"code":"CONFIG_INVALID","message":"sing-box check не прошёл"}}
```

## Dev-режим

```bash
cd app
python3 spike/helper/helper.py \
  --socket "$TMPDIR/splitroute-helper.sock" \
  --runtime-dir "$TMPDIR/splitroute-helper" \
  --sing-box ./src-tauri/binaries/sing-box-aarch64-apple-darwin
```

Сокет и runtime-каталог — во `$TMPDIR`, сервис не устанавливается. Остановить:
Ctrl+C (helper сам останавливает ядро и удаляет сокет).

## Файлы установки (черновики, НЕ устанавливались)

- `deploy/com.splitroute.helper.plist` — LaunchDaemon: `/Library/LaunchDaemons`,
  бинарь в `/Library/PrivilegedHelperTools`, сокет `/var/run/splitroute/helper.sock`.
- `deploy/splitroute-helper.service` — systemd: `RuntimeDirectory=splitroute`,
  `UMask=0077`, сокет `/run/splitroute/helper.sock`.

Установка, подпись и права проверяются на этапе 3. В этой сессии сервисы не
устанавливались, LaunchDaemon/systemd не активировались.

## Что НЕ проверено

- Реальная установка LaunchDaemon/systemd и работа под root — нужны права и
  подпись; не выполнялось.
- Проверка конфига с TUN под root — TUN в сессии недоступен.
- Поведение при падении ядра, автоперезапуск — не реализовано в прототипе
  (описано как задача этапа 3).
