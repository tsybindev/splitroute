#!/usr/bin/env sh
# Собирает переносимый архив SplitRoute для текущей ОС: распаковал и запустил,
# без установщика и без службы.
#
# Внутри — всё, что приложению нужно рядом с исполняемым файлом:
# приложение, `sing-box`, `splitroute-helper` и (на Windows) `wintun.dll`.
# Установщик ищет бинари именно рядом с приложением, поэтому архив работает
# без прав root: служба не регистрируется, TUN не поднимается.
#
# Windows: helper запускается в консоли от администратора подкомандой
# `console` (см. AGENTS.md §7). Без администратора TUN не работает.
#
# Сборка возможна только на целевой ОС: Tauri не кросс-компилирует приложение.
#
# Использование:
#   ./scripts/portable.sh            # архив для текущей платформы
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

if [ ! -d src-tauri/binaries ] || ! ls src-tauri/binaries/sing-box-* >/dev/null 2>&1; then
  echo "Нет sing-box в src-tauri/binaries. Выполните ./scripts/fetch-core.sh" >&2
  exit 1
fi

./scripts/build-helper-sidecar.sh

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    if [ ! -f src-tauri/binaries/wintun.dll ]; then
      echo "Нет wintun.dll. Выполните ./scripts/fetch-wintun.sh" >&2
      exit 1
    fi
    name=SplitRoute-portable-windows-x64
    ;;
  Linux)
    name=SplitRoute-portable-linux-x64
    ;;
  Darwin)
    name=SplitRoute-portable-macos-$(uname -m)
    ;;
  *)
    echo "Неподдерживаемая ОС: $(uname -s)" >&2
    exit 1
    ;;
esac

stage=$root/target/portable/$name
rm -rf "$stage"
mkdir -p "$stage"

# Слой с sidecar'ами: тот же контракт, что у платформенных конфигов.
overlay=$(mktemp "${TMPDIR:-/tmp}/splitroute-portable.XXXXXX.json")
trap 'rm -f "$overlay"' EXIT INT TERM
cat > "$overlay" <<'JSON'
{
  "bundle": {
    "active": false,
    "externalBin": ["binaries/sing-box", "binaries/splitroute-helper"]
  }
}
JSON

echo "Собираю приложение ($name)"
pnpm tauri build --no-bundle --config "$overlay"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    triple=x86_64-pc-windows-msvc
    cp "target/release/splitroute.exe" "$stage/"
    cp "src-tauri/binaries/sing-box-$triple.exe" "$stage/sing-box.exe"
    cp "src-tauri/binaries/splitroute-helper-$triple.exe" "$stage/splitroute-helper.exe"
    cp src-tauri/binaries/wintun.dll "$stage/"
    cat > "$stage/start.bat" <<'BAT'
@echo off
rem Запуск portable-сборки SplitRoute.
rem Helper требует прав администратора для TUN: окно запросит UAC.
setlocal
cd /d "%~dp0"

if not exist "splitroute-helper.exe" (
  echo Нет splitroute-helper.exe рядом со скриптом.
  exit /b 1
)

rem SID текущего пользователя: helper пускает к pipe только его.
for /f "delims=" %%S in ('powershell -NoProfile -Command "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value"') do set "USER_SID=%%S"
if "%USER_SID%"=="" (
  echo Не удалось определить SID пользователя.
  exit /b 1
)

echo Запускаю helper (окно UAC)...
powershell -NoProfile -Command ^
  "Start-Process -Verb RunAs -FilePath '.\splitroute-helper.exe' -ArgumentList 'console','--pipe','splitroute-helper','--runtime-dir','%~dp0runtime','--sing-box','%~dp0sing-box.exe','--allow-sid','%USER_SID%'"

timeout /t 2 /nobreak >nul
start "" "splitroute.exe"
endlocal
BAT
    ;;
  *)
    cp "target/release/splitroute" "$stage/SplitRoute"
    triple=$(rustc -vV | awk '/^host:/ {print $2}')
    cp "src-tauri/binaries/sing-box-$triple" "$stage/sing-box"
    cp "src-tauri/binaries/splitroute-helper-$triple" "$stage/splitroute-helper"
    ;;
esac

cp LICENSE "$stage/" 2>/dev/null || true

archive=$root/target/portable/$name.tar.gz
rm -f "$archive"
tar -czf "$archive" -C "$root/target/portable" "$name"
echo "Готово: $archive"
echo "Windows: распакуйте и запустите start.bat (UAC), затем splitroute.exe."
