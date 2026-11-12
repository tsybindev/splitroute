# Субагенты SplitRoute

Переносимые описания ролей для оркестратора. Формат — обычный Markdown с YAML-frontmatter (`name`, `description`, `tools`); лишние для конкретного харнесса поля игнорируются.

Источник истины — этот каталог. Харнесс-специфичные каталоги (например, `.commandcode/agents/`) создаются скриптом `scripts/sync-agents.sh` и в git не попадают:

```bash
./scripts/sync-agents.sh                      # Command Code: .commandcode/agents
./scripts/sync-agents.sh .claude/agents       # другой харнесс
```

| Роль | Зона ответственности |
|---|---|
| `rust-core` | Rust-ядро: подписка, модели, генератор конфига, состояние, тесты |
| `react-ui` | React/TS: экраны, компоненты, состояния, i18n |
| `platform-native` | helper, сокеты/пайпы, сервисы, TUN, упаковка под ОС |
| `verifier` | независимая проверка: diff, секреты, тесты, соответствие PRD |

Правила делегирования и оркестрации — в [AGENTS.md §18](../AGENTS.md#18-оркестрация-goal-и-субагенты).
