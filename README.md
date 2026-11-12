# SplitRoute (рабочее название)

Кроссплатформенный клиент для VLESS/Reality с выборочной маршрутизацией: в прокси уходят только выбранные приложения, остальной трафик идёт напрямую. Ядро — sing-box, UI — Tauri 2 + React + TypeScript, системная часть — Rust.

Целевые платформы: macOS, Windows, Linux (Arch/CachyOS, Ubuntu, Fedora).

## Документы

- [PRD.md](./PRD.md) — проблема, scope MVP, UX, требования, риски, roadmap.
- [AGENTS.md](./AGENTS.md) — стек, архитектура, поднятие среды разработки, соглашения, тестирование.
- [docs/research.md](./docs/research.md) — проверенные технические факты и ссылки.

Статус: этап 0 — документация. Код появляется на этапе 1 (spike sing-box + TUN + process-правила).
