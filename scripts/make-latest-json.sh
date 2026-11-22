#!/usr/bin/env sh
# Собирает манифест автообновления (latest.json / beta-latest.json) из
# подписанных артефактов `tauri build` (S9-T1).
#
# Формат статичного JSON tauri-plugin-updater: поле `signature` — base64
# содержимого .sig (плагин декодирует base64 и парсит minisign-подпись:
# base64_to_string → Signature::decode), поле `url` — прямая ссылка на ассет
# релиза. Ассеты GitHub Releases плоские, поэтому канал кодируется именем
# файла манифеста (latest.json — stable, beta-latest.json — beta),
# как в src-tauri/src/updates.rs. Версия в манифесте сверяется апдейтером
# с `version:` в trusted comment подписи (requireSignedVersion).
#
# Использование:
#   scripts/make-latest-json.sh --version 0.1.0 --tag v0.1.0 \
#     --repo tsybindev/splitroute [--notes "текст"] [--out latest.json] \
#     [--require-platform windows-x86_64,linux-x86_64]
#
# Платформы берутся из артефактов версии в target/release/bundle:
#   windows-x86_64 — nsis/*_<version>_*-setup.exe + .sig,
#   linux-x86_64   — appimage/*_<version>_*.AppImage + .AppImage.sig.
# deb/rpm апдейтером не используются и в манифест не попадают.
# JSON собирается node: в манифесте многострочные поля (notes), а jq нет
# ни на windows-runner'ах, ни гарантированно локально.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

version=
tag=
repo=
notes=
out=latest.json
require_platform=

while [ $# -gt 0 ]; do
  case "$1" in
    --version) version=$2; shift 2 ;;
    --tag) tag=$2; shift 2 ;;
    --repo) repo=$2; shift 2 ;;
    --notes) notes=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --require-platform) require_platform=$2; shift 2 ;;
    *) echo "Неизвестный аргумент: $1" >&2; exit 1 ;;
  esac
done

if [ -z "$version" ] || [ -z "$tag" ] || [ -z "$repo" ]; then
  echo "Использование: $0 --version 0.1.0 --tag v0.1.0 --repo owner/name [--notes т] [--out файл] [--require-platform список]" >&2
  exit 1
fi
case "$tag" in
  v*) ;;
  *) echo "Тег \"$tag\": ожидается префикс v" >&2; exit 1 ;;
esac

node - --version "$version" --tag "$tag" --repo "$repo" \
  --notes "${notes:-SplitRoute $version}" --out "$out" \
  --require-platform "${require_platform:-}" <<'NODE'
const fs = require('fs');
const path = require('path');

const argv = process.argv.slice(2);
const args = {};
for (let i = 0; i + 1 < argv.length; i += 2) {
  if (!argv[i].startsWith('--')) throw new Error(`ожидались пары --ключ значение: ${argv[i]}`);
  args[argv[i].slice(2)] = argv[i + 1];
}
for (const key of ['version', 'tag', 'repo', 'notes', 'out']) {
  if (!args[key]) throw new Error(`нет обязательного --${key}`);
}

const bundle = path.join('target', 'release', 'bundle');
// В bundle могут лежать артефакты прошлых сборок: берём только версию релиза.
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const versionRe = new RegExp(`_${esc(args.version)}_`);

// Ищет единственный .sig версии в подкаталоге bundle; возвращает путь
// к подписи и имя артефакта (имя .sig минус хвост ".sig").
function resolvePlatform(dir, sigSuffix) {
  if (!fs.existsSync(dir)) return null;
  const files = fs.readdirSync(dir).filter((f) => versionRe.test(f) && f.endsWith(sigSuffix)).sort();
  if (files.length === 0) return null;
  if (files.length > 1) throw new Error(`в ${dir} несколько артефактов версии ${args.version}: ${files.join(', ')}`);
  const sigPath = path.join(dir, files[0]);
  // В манифесте плагин ожидает base64(minisign-текст): поле декодируется
  // base64_to_string и парсится как minisign. CLI v2 пишет .sig уже в base64
  // (одна строка) — берём как есть; сырой текст (v1) кодируем в base64.
  const raw = fs.readFileSync(sigPath, 'utf8').trim();
  const isText = raw.startsWith('untrusted comment:');
  const content = isText ? Buffer.from(raw, 'utf8').toString('base64') : raw;
  const minisign = isText ? raw : Buffer.from(raw, 'base64').toString('utf8');
  if (!minisign.startsWith('untrusted comment:')) {
    throw new Error(`${sigPath} не похож на minisign-подпись`);
  }
  const signedVersion = /(?:^|\t)version:([^\t\n]*)/.exec(minisign);
  if (signedVersion && signedVersion[1].trim() !== args.version) {
    throw new Error(`${sigPath}: подписана версия ${signedVersion[1].trim()}, ожидается ${args.version}`);
  }
  const artifact = files[0].slice(0, -'.sig'.length);
  return {
    signature: content,
    url: `https://github.com/${args.repo}/releases/download/${args.tag}/${artifact}`,
  };
}

const found = {
  'windows-x86_64': resolvePlatform(path.join(bundle, 'nsis'), '-setup.exe.sig'),
  'linux-x86_64': resolvePlatform(path.join(bundle, 'appimage'), '.AppImage.sig'),
};

for (const platform of (args['require-platform'] || '').split(',').filter(Boolean)) {
  if (!found[platform]) {
    throw new Error(`нет подписанного артефакта платформы ${platform} (версия ${args.version}) в ${bundle}`);
  }
}

const platforms = {};
for (const [platform, entry] of Object.entries(found)) {
  if (entry) platforms[platform] = entry;
}
if (Object.keys(platforms).length === 0) {
  throw new Error(`в ${bundle} нет подписанных артефактов версии ${args.version}`);
}

const manifest = {
  version: args.version,
  notes: args.notes,
  pub_date: new Date().toISOString(),
  platforms,
};
fs.writeFileSync(args.out, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Манифест: ${args.out} — платформы ${Object.keys(platforms).join(', ')}`);
NODE
