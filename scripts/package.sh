#!/usr/bin/env sh
# Собирает пакет SplitRoute для текущей платформы.
#
# Tauri не кросс-собирает установщики: `.deb`/`.rpm`/AppImage делаются на Linux,
# NSIS — на Windows, `.app`/`.dmg` — на macOS. Скрипт проверяет платформу,
# готовит sidecar helper'а и запускает `tauri build`.
#
# Бинари helper'а и sing-box лежат в `src-tauri/binaries` и в git не попадают,
# поэтому `externalBin` задан только в платформенных конфигах
# (`tauri.linux.conf.json`, `tauri.windows.conf.json`). Для macOS слой
# передаётся через временный конфиг: базовый `tauri.conf.json` остаётся без
# `externalBin`, иначе `cargo check` на чистом клоне падал бы на отсутствующих
# файлах.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

if [ ! -d src-tauri/binaries ] || ! ls src-tauri/binaries/sing-box-* >/dev/null 2>&1; then
  echo "Нет sing-box в src-tauri/binaries. Выполните ./scripts/fetch-core.sh" >&2
  exit 1
fi

./scripts/build-helper-sidecar.sh

case "$(uname -s)" in
  Linux)
    echo "Пакет: deb, rpm, appimage"
    exec pnpm tauri build --bundles deb,rpm,appimage --config src-tauri/tauri.linux.conf.json
    ;;
  Darwin)
    # Временный слой с externalBin: удаляется при выходе.
    overlay=$(mktemp "${TMPDIR:-/tmp}/splitroute-bundle.XXXXXX.json")
    trap 'rm -f "$overlay"' EXIT INT TERM
    cat > "$overlay" <<'JSON'
{
  "bundle": {
    "active": true,
    "targets": ["app", "dmg"],
    "externalBin": ["binaries/sing-box", "binaries/splitroute-helper"]
  }
}
JSON
    echo "Пакет: app, dmg"
    pnpm tauri build --bundles app,dmg --config "$overlay"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    if [ ! -f src-tauri/binaries/wintun.dll ]; then
      echo "Нет wintun.dll. Выполните ./scripts/fetch-wintun.sh" >&2
      exit 1
    fi
    echo "Пакет: nsis"
    exec pnpm tauri build --bundles nsis --config src-tauri/tauri.windows.conf.json
    ;;
  *)
    echo "Неподдерживаемая ОС для сборки пакета: $(uname -s)" >&2
    exit 1
    ;;
esac
