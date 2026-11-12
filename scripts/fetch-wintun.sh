#!/usr/bin/env sh
# Скачивает официальную подписанную библиотеку wintun и кладёт её в
# src-tauri/binaries/wintun.dll.
#
# wintun — это драйвер TUN для Windows; собирать его из исходников нельзя,
# нужна подписанная сборка с wintun.net (AGENTS.md §7). Библиотека должна
# лежать рядом с sing-box.exe, иначе ядро не поднимет TUN.
#
# Использование:
#   ./scripts/fetch-wintun.sh
set -eu

WINTUN_VERSION=0.14.1
# sha256 архива с https://www.wintun.net/builds/wintun-<версия>.zip
WINTUN_SHA256=07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cache_dir=$root/.cache/wintun
bin_dir=$root/src-tauri/binaries
asset=wintun-${WINTUN_VERSION}.zip
url=https://www.wintun.net/builds/${asset}

# Каталог внутри архива: bin/<arch>/wintun.dll.
case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  arm64|aarch64) arch=arm64 ;;
  *) echo "Неподдерживаемая архитектура: $(uname -m)" >&2; exit 1 ;;
esac

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "Нужен sha256sum или shasum" >&2
    exit 1
  fi
}

mkdir -p "$cache_dir" "$bin_dir"
archive=$cache_dir/$asset

if [ -f "$archive" ]; then
  if [ "$(sha256_of "$archive")" != "$WINTUN_SHA256" ]; then
    echo "Кэш повреждён, перекачиваю $asset" >&2
    rm -f "$archive"
  fi
fi

if [ ! -f "$archive" ]; then
  echo "Скачиваю $url"
  curl -fsSL --retry 3 --max-time 300 -o "$archive.part" "$url"
  actual=$(sha256_of "$archive.part")
  if [ "$actual" != "$WINTUN_SHA256" ]; then
    rm -f "$archive.part"
    echo "SHA256 не совпал: $actual != $WINTUN_SHA256" >&2
    exit 1
  fi
  mv "$archive.part" "$archive"
  echo "SHA256 совпал: $asset"
fi

extract_dir=$cache_dir/extract-${WINTUN_VERSION}-${arch}
rm -rf "$extract_dir"
mkdir -p "$extract_dir"

if ! command -v unzip >/dev/null 2>&1; then
  echo "Для zip нужен unzip" >&2
  exit 1
fi
unzip -q "$archive" -d "$extract_dir"

src=$extract_dir/wintun/bin/$arch/wintun.dll
if [ ! -f "$src" ]; then
  echo "В архиве не найден wintun/bin/$arch/wintun.dll" >&2
  exit 1
fi

cp "$src" "$bin_dir/wintun.dll"
echo "Готово: $bin_dir/wintun.dll (wintun $WINTUN_VERSION, $arch)"
