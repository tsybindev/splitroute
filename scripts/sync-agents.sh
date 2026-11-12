#!/usr/bin/env sh
# Синхронизирует переносимые описания субагентов в каталог, который читает харнесс.
# По умолчанию — .commandcode/agents (Command Code). Другой харнесс:
#   ./scripts/sync-agents.sh .claude/agents
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${1:-.commandcode/agents}

case "$target" in
  /*) dest=$target ;;
  *) dest=$root/$target ;;
esac

mkdir -p "$dest"
for file in "$root"/agents/*.md; do
  name=$(basename "$file")
  [ "$name" = "README.md" ] && continue
  cp "$file" "$dest/$name"
done

echo "Агенты синхронизированы: $dest"
