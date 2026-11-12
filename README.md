# SplitRoute

Кроссплатформенный клиент для VLESS/Reality с выборочной маршрутизацией: в прокси уходят только выбранные приложения, остальной трафик идёт напрямую. Ядро — sing-box, UI — Tauri 2 + React + TypeScript, системная часть — Rust.

Целевые платформы: macOS, Windows, Linux (Arch/CachyOS, Ubuntu, Fedora). Лицензия — GPL-3.0.

## Документы

- [PRD.md](./PRD.md) — проблема, scope MVP, UX, требования, риски, roadmap.
- [AGENTS.md](./AGENTS.md) — стек, архитектура, поднятие среды разработки, соглашения, тестирование, правила оркестрации.
- [agents/](./agents/README.md) — переносимые роли субагентов для любого харнесса.
- [TASKS.md](./TASKS.md) — реестр этапов и задач с критериями приёмки.
- [NEXT_SESSION.md](./NEXT_SESSION.md) — состояние и готовый промпт для следующей сессии.
- [docs/research.md](./docs/research.md) — проверенные технические факты и ссылки.

## Статус

Этапы 0 и 1 завершены: документация, спайки и отчёты в `docs/spikes/`.
Этап 2 (ядро MVP) завершён: подписка, локации, allowlist, подключение через
helper, трей и экраны; реальный TUN, внешний IP и очистка проверены на macOS.
Разделение DNS по процессу на macOS 15 / sing-box 1.14 не поддерживается.
Решение: пользователь выбирает режим `proxy` (весь DNS через прокси, по
умолчанию) или `system` (DNS ОС, с предупреждением); реализация — S2-T10.
Подробности — в [docs/stage-2-report.md](./docs/stage-2-report.md).
Этап 3: helper'ы под три ОС, установка из UI с показом причины отказа и
упаковка с `LICENSE` и [THIRD_PARTY_NOTICES](./THIRD_PARTY_NOTICES) в бандле.
Осталось: smoke на Linux/Windows (S3-T7). Детали —
[docs/stage-3-report.md](./docs/stage-3-report.md).
