# NEXT_SESSION.md — handoff

> Файл перезаписывается в конце каждого этапа или подцели. Ниже — состояние и готовый промпт для следующей сессии.

## Состояние

- Этап 0 завершён. Этап 1 в работе: подцель `S1-T1…S1-T3` закрыта.
- Последний коммит: `3a7bb98` — `feat: добавлены pinned sing-box, фикстура подписки и конвертер в sing-box`.
- Ветка: `main`. Репозиторий: https://github.com/tsybindev/splitroute (private).
- Сделано:
  - `scripts/core-version.txt` — pinned sing-box **1.14.4**, sha256 по darwin/linux/windows × arm64/amd64.
  - `scripts/fetch-core.sh` — скачивание, проверка sha256 до распаковки, кэш в `.cache/core/`, распаковка в `src-tauri/binaries/sing-box-<triple>`, сверка `sing-box version`. Есть `--verify`.
  - `spike/fixtures/subscription.json` — 2 профиля VLESS/Reality/TCP, служебные outbound'ы `direct`, `block`, `fragment-out`; все значения синтетические (TEST-NET-3, `example.com`/`example.net`).
  - `spike/convert.py` — Xray → sing-box outbound + полный конфиг (TUN, mixed, DNS, route). Оба конфига проходят `sing-box check`.
  - `spike/test_convert.py` — 10 тестов (stdlib unittest): маппинг, пропуски, golden-воспроизводимость, ошибки входа.
  - `spike/configs/profile-1-*.json`, `profile-2-*.json` — проверенные ядром конфиги.
- Не сделано: `S1-T4…S1-T12` (см. `TASKS.md`).
- Окружение: macOS 15.8.2 arm64, Node 22, pnpm, Python 3.14, бинарь ядра скачан в `src-tauri/binaries/` (в git не попадает).
- Реальная подписка не скачивалась: `SUBSCRIPTION_URL`/`SUBSCRIPTION_HWID` в сессии не заданы, в репозитории их нет.

### Важные факты для следующей сессии

- Готовый вход для проверок TUN/process-правил: `spike/configs/profile-1-testovaya-lokatsiya-a.json` и `profile-2-*.json`. В route уже есть правила `process_name: ["curl"]` и `process_path_regex` для Chrome, `final: direct`.
- Ядро: `./src-tauri/binaries/sing-box-aarch64-apple-darwin` (1.14.4). Для TUN на macOS нужен `sudo`.
- Адреса в конфиге — TEST-NET-3 (`203.0.113.10:443`), реального сервера нет. Для живого теста нужен реальный профиль: положить его во временный файл вне репозитория (см. `spike/fixtures/README.md`) и прогнать `python3 spike/convert.py --input <tmp> --out-dir <tmp> --check`.
- **Секреты не коммитить.** Реальный профиль — только во `$TMPDIR`, после проверки удалить.

### Что проверять первым делом

```bash
cd app
./scripts/fetch-core.sh --verify
python3 -m unittest discover -s spike -p 'test_*.py'
./src-tauri/binaries/sing-box-aarch64-apple-darwin check -c spike/configs/profile-1-testovaya-lokatsiya-a.json
```

---

## Промпт следующей сессии

Скопируй текст ниже в новую сессию Command Code. Запускай сессию из каталога `app/`, чтобы подхватился `AGENTS.md`.

```text
Ты — оркестратор разработки проекта SplitRoute (per-app VPN клиент на Tauri 2 + React + Rust + sing-box).

Рабочий каталог: /Users/tsybin_d/Documents/Dev/Backend/vpn_client/app
Цель текущей сессии: выполнить S1-T4…S1-T7 из TASKS.md — проверить TUN и process-правила на macOS 15, macOS 26/27, CachyOS и Windows VM. Остановись после коммита отчётов и обновления NEXT_SESSION.md.

Сначала прочитай:
- AGENTS.md (§10 архитектура, §13 DoD, §14 правила для агентов, §18 оркестрация);
- PRD.md §6.2 (TUN и маршрутизация), §6.3 (allowlist), §10 риски R1, R2, R3, R4;
- TASKS.md (S1-T4…S1-T7 и их критерии приёмки);
- docs/research.md (§1, §2, §3 — per-app на macOS, mihomo #3355, NetworkExtension);
- spike/README.md, spike/fixtures/README.md, spike/convert.py.

Что уже готово:
- pinned sing-box 1.14.4 скачивается через ./scripts/fetch-core.sh, бинарь лежит в src-tauri/binaries/;
- конвертер spike/convert.py и проверенные конфиги spike/configs/*.json (TUN + process_name curl + process_path_regex Chrome + mixed 127.0.0.1:10808);
- тесты: python3 -m unittest discover -s spike -p 'test_*.py'.

Задачи:
1. S1-T4 — macOS 15.x: поднять TUN через sing-box, проверить, что curl (allowlist) идёт через прокси, а невыбранный процесс — напрямую, по внешнему IP. Зафиксировать в spike/macos/README.md методику и логи.
2. S1-T5 — macOS 26/27: проверить, воспроизводится ли ограничение R1 (process-правила у не-Apple-подписанных бинарей). Если ОС недоступна — честно записать «не проверено» с причиной.
3. S1-T6 — CachyOS: тот же эксперимент, зафиксировать пакеты, права, TUN и результаты.
4. S1-T7 — Windows VM (если доступна): TUN + wintun.dll + process-правила; иначе зафиксировать отсутствие среды.
5. Обнови TASKS.md (статусы S1-T4…S1-T7) и NEXT_SESSION.md для следующей подцели S1-T8…S1-T10.

Ограничения:
- Продакшн-код не пишем: только spike/ и docs/spikes/.
- Реальную подписку не коммитим: SUBSCRIPTION_URL и SUBSCRIPTION_HWID берутся из окружения, реальные UUID/адреса — только во $TMPDIR и удаляются после проверки. В отчётах — обезличенные наблюдения.
- Не оставлять запущенные TUN, sing-box и helper-процессы; после экспериментов всё останавливать.
- Не выполнять разрушительные команды; не менять архитектурные решения PRD без обсуждения.
- Комментарии в коде на русском, идентификаторы английские.
- Субагенты не коммитят и не правят PRD/AGENTS/TASKS/NEXT_SESSION — это делает только оркестратор.

Как работать:
1. Зафиксируй методику проверки (как измеряем внешний IP для выбранного и невыбранного процесса) до запуска.
2. Параллельно запускай не больше 3 субагентов без пересечения файлов; macOS и Linux эксперименты — в разных каталогах (spike/macos, spike/linux, spike/windows).
3. После сборки результатов прогони проверки и вручную подтверди поведение на macOS.
4. Коммит(ы) с отчётами и скриптами spike, затем отдельный коммит docs: с TASKS.md и NEXT_SESSION.md.
5. Обнови NEXT_SESSION.md под S1-T8…S1-T10 и остановись.

Критерии готовности подцели:
- Для каждой доступной ОС есть воспроизводимая методика и логи.
- Явно зафиксировано, работают ли process-правила на macOS 15.x и на 26/27.
- Недоступные среды помечены как непроверенные, без выдуманных результатов.
- Секретов в репозитории нет, запущенных TUN/процессов не осталось.
```

---

## Рекомендуемые цели для этапа 1

Не запускай весь этап одной целью — лимит итераций `/goal` фиксированный. Иди последовательно: после каждой цели коммит и обновление этого файла, затем `/goal clear` и следующая цель.

```text
/goal Выполни S1-T4…S1-T7 из TASKS.md: проверка TUN и process-правил на macOS 15, macOS 26/27, CachyOS и Windows VM. Остановись после коммита отчётов и обновления NEXT_SESSION.md.
```

```text
/goal Выполни S1-T8…S1-T10 из TASKS.md: DNS-политика, QUIC/UDP и прототип helper'а. Остановись после коммита кода и обновления NEXT_SESSION.md.
```

```text
/goal Выполни S1-T11…S1-T12 из TASKS.md: итоговый отчёт этапа 1, go/no-go по платформам, обновление рисков PRD и handoff для этапа 2. Остановись после коммита документации.
```
