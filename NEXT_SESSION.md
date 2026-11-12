# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждой подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 1 в работе. Закрыты `S1-T1…S1-T3` (раньше) и подцель G1 `S1-T4…S1-T7` (сейчас).
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Последние коммиты: код стенда G1 (`feat:`), документация G1 (`docs:`).

### Сделано в G1 (S1-T4…S1-T7)

- `spike/macos/process_rules_test.py` — стенд без root: mixed-inbound + два
  фейковых SOCKS5-outbound с разными маркерами ответа.
- `spike/macos/socksprobe.go` — SOCKS5-клиент; копируется под разными именами и
  путями (системный `curl` копировать нельзя: restricted binary убивается AMFI).
- `spike/macos/README.md` — методика, результаты, ограничения.
- **Подтверждено на macOS 15.8.2 arm64, sing-box 1.14.4:** `process_name` и
  `process_path_regex` для TCP; helper-процессы Chrome матчатся по каталогу `.app`
  (риск R3 подтверждён и митигируется правилом по пути).
- **Не проверено:** TUN (нет root), сравнение внешнего IP (нет реальной подписки),
  macOS 26/27 (нет машины), CachyOS (нет машины, Docker-демон не запущен),
  Windows (нет VM).

### Важные факты окружения

- macOS 15.8.2 (24H232), arm64; `sudo -n` требует пароль → TUN недоступен.
- `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID` не заданы; реального сервера нет.
- `override_address` в direct-outbound удалён в sing-box 1.13+; для наблюдаемости
  используются `socks`-outbound'ы.
- На машине активен сторонний VPN (v2RayTun) и `utun0…utun4` — это риск R5, но на
  rootless-тест не влияет.
- Go 1.27.0 установлен (нужен для `socksprobe.go`); Rust/cargo отсутствует.

### Проверка перед стартом

```bash
cd app
git status --short && git log --oneline -3
./scripts/fetch-core.sh --verify
python3 -m unittest discover -s spike -p 'test_*.py'
python3 spike/macos/process_rules_test.py --no-chrome
pgrep -fl 'sing-box run' || echo "sing-box не запущен"
```

---

## Промпт следующей подцели: G2 (S1-T8…S1-T10)

Скопируй текст ниже целиком в новую сессию. Запускай из каталога `app/`.

```text
Ты — оркестратор разработки SplitRoute (per-app VPN на Tauri 2 + React + Rust + sing-box).
Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app

ЦЕЛЬ: выполнить подцель G2 этапа 1 — S1-T8 (DNS-политика), S1-T9 (QUIC/UDP),
S1-T10 (прототип helper'а). После подцели остановись: коммит кода, отдельный коммит
документации, обновление NEXT_SESSION.md. Следующую подцель G3 не начинай.

ПРОЧИТАЙ СНАЧАЛА
- AGENTS.md §10 (архитектура), §12 (git), §13 (DoD), §14 (правила агентов), §18 (лимиты /goal).
- PRD.md §6.2 (TUN/маршрутизация), §6.4 (DNS), §6.8 (helper), §10 (R4, R6).
- TASKS.md: S1-T8, S1-T9, S1-T10.
- docs/research.md §7.
- NEXT_SESSION.md (этот файл) и spike/macos/README.md — что уже проверено и каким способом.

СОСТОЯНИЕ
- G1: process-правила на macOS 15.8.2 подтверждены без root (spike/macos/).
- TUN не поднимается: нет root (sudo требует пароль). Реальной подписки нет.
- macOS 26/27, CachyOS, Windows VM недоступны.

ОГРАНИЧЕНИЯ, КОТОРЫЕ НЕЛЬЗЯ ОБХОДИТЬ
- Не выдавай непроверенное за проверенное. Недоступная ОС/права — «не проверено» с причиной.
- Не запускай необратимые системные изменения. LaunchDaemon/systemd-юниты в этой
  сессии НЕ устанавливаются: готовятся файлы юнитов и dev-режим без установки.
- Секреты не коммитить; реальная подписка только из SUBSCRIPTION_URL/SUBSCRIPTION_HWID во $TMPDIR.

ЗАДАЧИ

S1-T8, DNS-политика.
- Проверь на уровне конфига и запуска без TUN: принимает ли sing-box 1.14.4
  dns-правила с process_name/process_path_regex, и что происходит с запросом.
- DNS через TUN (перехват) без root не проверяется — зафиксируй как непроверенное.
- Опиши рабочий вариант и fallback из PRD §6.4 (remote DoT через proxy для выбранных,
  local для остальных; fallback — системный DNS с предупреждением в UI).
- systemd-resolved и Private Relay: без Linux/macOS 26 — только гипотезы, помечай явно.

S1-T9, QUIC/UDP.
- Проверь, проходит ли UDP через mixed-inbound sing-box и применяются ли к нему
  process-правила (SOCKS5 UDP ASSOCIATE). Если слишком дорого — сделай минимальный
  UDP-эхо тест и зафиксируй результат.
- Сформулируй решение: пропускать / блокировать / direct, с обоснованием и ссылкой
  на риск R4. Не подгоняй вывод под PRD.

S1-T10, прототип helper'а.
- В `spike/helper/` сделай минимальный прототип: Unix socket с правами 0600,
  владелец — пользователь, проверка peer uid, команды только start/stop/status,
  лимит размера сообщения, версия протокола, таймаут.
- Helper запускает/останавливает sing-box (без TUN, конфиг с mixed-inbound).
- Dev-режим: запуск под обычным пользователем, сокет в $TMPDIR, без установки сервиса.
- Подготовь (но НЕ устанавливай) файлы: macOS LaunchDaemon plist и Linux systemd unit;
  опиши пути, права, сокет в /run/splitroute/ и /Library/LaunchDaemons.
- Если Rust недоступен (cargo нет), прототип допустим на Python/Go в spike/ — это
  спайк, не продакшн; в README укажи, что продакшн-helper будет на Rust.

ВНУТРИ ПОДЦЕЛИ
1. Сначала зафиксируй методику: команды, процессы, как измеряется результат.
2. Выполни работу в spike/ (dns/, udp/, helper/ или рядом).
3. Прогони проверки: sing-box check, тесты конвертера, запуск прототипа, отсутствие
   зависших процессов/TUN.
4. Коммит кода: feat:/test:. Затем отдельный коммит docs: с TASKS.md и NEXT_SESSION.md.
5. Обнови TASKS.md (статусы S1-T8…S1-T10) и NEXT_SESSION.md (состояние + промпт G3).

ОТЧЁТ В КОНЦЕ
- Что сделано, какие файлы.
- Как проверено (команды + результат).
- Что не проверено и почему.
- Чистота системы: что остановлено/удалено.
```

---

## Открытые вопросы для G3 (отчёт)

- Итоговый отчёт `docs/spikes/stage-1.md` должен свести: process-правила (macOS 15),
  TUN (не проверено), DNS, QUIC, helper, ограничения macOS 26/27/Linux/Windows.
- Обновить риски PRD по фактическим результатам, не подгоняя их под исходные.
