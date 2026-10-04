#!/usr/bin/env bash
# Sets the app version in package.json, src-tauri/Cargo.toml and src-tauri/tauri.conf.json.
# Usage: scripts/bump-version.sh 1.0.0      (check only: scripts/bump-version.sh --check v1.0.0)
set -euo pipefail
cd "$(dirname "$0")/.."

current() {
  node -p "require('./package.json').version"
  grep -m1 '^version = ' src-tauri/Cargo.toml | sed -E 's/version = "(.*)"/\1/'
  node -p "require('./src-tauri/tauri.conf.json').version"
}

if [ "${1:-}" = "--check" ]; then
  want="${2#v}"
  bad=0
  for v in $(current); do [ "$v" = "$want" ] || bad=1; done
  if [ $bad -ne 0 ]; then
    echo "version mismatch: tag $want vs $(current | tr '\n' ' ')" >&2
    exit 1
  fi
  echo "version $want OK"
  exit 0
fi

new="${1:?usage: bump-version.sh <x.y.z>}"
[[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not semver: $new" >&2; exit 1; }
node -e "
const fs=require('fs');
for (const f of ['package.json','src-tauri/tauri.conf.json']) {
  const j=JSON.parse(fs.readFileSync(f,'utf8')); j.version='$new';
  fs.writeFileSync(f, JSON.stringify(j,null,2)+'\n');
}
const l=JSON.parse(fs.readFileSync('package-lock.json','utf8')); l.version='$new'; if (l.packages&&l.packages['']) l.packages[''].version='$new';
fs.writeFileSync('package-lock.json', JSON.stringify(l,null,2)+'\n');"
sed -i -E '0,/^version = ".*"/s//version = "'"$new"'"/' src-tauri/Cargo.toml
(cd src-tauri && cargo update -p yts-player --offline >/dev/null 2>&1 || true)
echo "version → $new"; current
