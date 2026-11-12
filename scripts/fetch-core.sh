#!/usr/bin/env sh
# Скачивает закреплённую версию sing-box, проверяет SHA256 и кладёт бинарь
# в src-tauri/binaries/ под именем, которое ожидает Tauri sidecar.
#
# Использование:
#   ./scripts/fetch-core.sh            # скачать под текущую платформу
#   ./scripts/fetch-core.sh --verify   # только проверить уже скачанный архив
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version_file=$root/scripts/core-version.txt
cache_dir=$root/.cache/core
bin_dir=$root/src-tauri/binaries

if [ ! -f "$version_file" ]; then
  echo "Не найден $version_file" >&2
  exit 1
fi

# shellcheck disable=SC1090
. "$version_file"

if [ -z "${SING_BOX_VERSION:-}" ]; then
  echo "В $version_file не задан SING_BOX_VERSION" >&2
  exit 1
fi

case "$(uname -s)" in
  Darwin) os=darwin ;;
  Linux) os=linux ;;
  MINGW*|MSYS*|CYGWIN*) os=windows ;;
  *) echo "Неподдерживаемая ОС: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  arm64|aarch64) arch=arm64 ;;
  *) echo "Неподдерживаемая архитектура: $(uname -m)" >&2; exit 1 ;;
esac

# Имя target triple для Tauri sidecar (externalBin добавляет суффикс).
case "$os-$arch" in
  darwin-arm64)  triple=aarch64-apple-darwin ;;
  darwin-amd64)  triple=x86_64-apple-darwin ;;
  linux-arm64)   triple=aarch64-unknown-linux-gnu ;;
  linux-amd64)   triple=x86_64-unknown-linux-gnu ;;
  windows-arm64) triple=aarch64-pc-windows-msvc ;;
  windows-amd64) triple=x86_64-pc-windows-msvc ;;
esac

# Имя архива в официальном релизе.
if [ "$os" = windows ]; then
  ext=zip
else
  ext=tar.gz
fi
asset=sing-box-${SING_BOX_VERSION}-${os}-${arch}.${ext}
url=https://github.com/SagerNet/sing-box/releases/download/v${SING_BOX_VERSION}/${asset}

# Хэш из core-version.txt по ключу sha256_<os>_<arch>. Без eval: значения фиксированы.
case "${os}_${arch}" in
  darwin_arm64)  expected=${sha256_darwin_arm64:-} ;;
  darwin_amd64)  expected=${sha256_darwin_amd64:-} ;;
  linux_arm64)   expected=${sha256_linux_arm64:-} ;;
  linux_amd64)   expected=${sha256_linux_amd64:-} ;;
  windows_arm64) expected=${sha256_windows_arm64:-} ;;
  windows_amd64) expected=${sha256_windows_amd64:-} ;;
  *)             expected= ;;
esac
if [ -z "$expected" ]; then
  echo "В $version_file нет sha256_${os}_${arch}" >&2
  exit 1
fi

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

if [ "${1:-}" = "--verify" ]; then
  if [ ! -f "$archive" ]; then
    echo "Архив не найден: $archive" >&2
    exit 1
  fi
  actual=$(sha256_of "$archive")
  if [ "$actual" != "$expected" ]; then
    echo "SHA256 не совпал для $archive: $actual != $expected" >&2
    exit 1
  fi
  echo "SHA256 совпал: $asset"
  exit 0
fi

if [ -f "$archive" ]; then
  actual=$(sha256_of "$archive")
  if [ "$actual" != "$expected" ]; then
    echo "Кэш повреждён, перекачиваю $asset" >&2
    rm -f "$archive"
  fi
fi

if [ ! -f "$archive" ]; then
  echo "Скачиваю $url"
  curl -fsSL --retry 3 --max-time 300 -o "$archive.part" "$url"
  actual=$(sha256_of "$archive.part")
  if [ "$actual" != "$expected" ]; then
    rm -f "$archive.part"
    echo "SHA256 не совпал: $actual != $expected" >&2
    exit 1
  fi
  mv "$archive.part" "$archive"
  echo "SHA256 совпал: $asset"
fi

extract_dir=$cache_dir/extract-${SING_BOX_VERSION}-${os}-${arch}
rm -rf "$extract_dir"
mkdir -p "$extract_dir"

if [ "$ext" = zip ]; then
  if ! command -v unzip >/dev/null 2>&1; then
    echo "Для zip нужен unzip" >&2
    exit 1
  fi
  unzip -q "$archive" -d "$extract_dir"
else
  tar -xzf "$archive" -C "$extract_dir"
fi

if [ "$os" = windows ]; then
  src=$(find "$extract_dir" -type f -name 'sing-box.exe' | head -n 1)
  dest=$bin_dir/sing-box-$triple.exe
else
  src=$(find "$extract_dir" -type f -name 'sing-box' | head -n 1)
  dest=$bin_dir/sing-box-$triple
fi

if [ -z "$src" ]; then
  echo "В архиве не найден бинарь sing-box" >&2
  exit 1
fi

cp "$src" "$dest"
chmod +x "$dest"

reported=$("$dest" version 2>/dev/null | head -n 1 | awk '{print $3}')
if [ "$reported" != "$SING_BOX_VERSION" ]; then
  echo "Версия бинаря не совпала: '$reported' != '$SING_BOX_VERSION'" >&2
  exit 1
fi

echo "Готово: $dest (sing-box $reported)"
