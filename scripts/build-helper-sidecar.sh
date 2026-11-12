#!/usr/bin/env sh
# Готовит sidecar-бинарь helper'а для Tauri-бандла.
#
# Tauri ждёт `src-tauri/binaries/splitroute-helper-<target-triple>[.exe]`, а
# `cargo build -p splitroute-helper` кладёт бинарь без суффикса. Скрипт собирает
# helper и раскладывает его под именем, которое понимает `externalBin`.
#
# Использование:
#   ./scripts/build-helper-sidecar.sh                 # текущая платформа
#   ./scripts/build-helper-sidecar.sh --target <triple>
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
bin_dir=$root/src-tauri/binaries

target=""
if [ "${1:-}" = "--target" ]; then
  target=${2:?--target требует значение}
fi

build_args=""
if [ -n "$target" ]; then
  build_args="--target $target"
  triple=$target
else
  triple=$(rustc -vV | awk '/^host:/ {print $2}')
fi

# Windows-сборка даёт .exe; имя sidecar тоже должно быть с .exe.
case "$triple" in
  *windows*) ext=.exe ;;
  *) ext= ;;
esac

echo "Собираю splitroute-helper ($triple)"
# shellcheck disable=SC2086
cargo build -p splitroute-helper --release $build_args

if [ -n "$target" ]; then
  built=$root/target/$triple/release/splitroute-helper$ext
else
  built=$root/target/release/splitroute-helper$ext
fi

if [ ! -f "$built" ]; then
  echo "Не найден собранный helper: $built" >&2
  exit 1
fi

mkdir -p "$bin_dir"
dest=$bin_dir/splitroute-helper-$triple$ext
cp "$built" "$dest"
chmod +x "$dest"
echo "Готово: $dest"
echo "sing-box в бандл берётся из scripts/fetch-core.sh (externalBin: binaries/sing-box)."
