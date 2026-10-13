#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: bump-version.sh X.Y.Z | bump-version.sh --check" >&2
  exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_JSON="$ROOT/package.json"
CARGO_TOML="$ROOT/src-tauri/Cargo.toml"
TAURI_CONF="$ROOT/src-tauri/tauri.conf.json"

get_versions() {
  python3 - "$PKG_JSON" "$CARGO_TOML" "$TAURI_CONF" <<'EOF'
import json, re, sys
pkg_json, cargo_toml, tauri_conf = sys.argv[1], sys.argv[2], sys.argv[3]
with open(pkg_json) as f:
    pkg_version = json.load(f)["version"]
with open(cargo_toml) as f:
    m = re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.M)
    cargo_version = m.group(1) if m else "MISSING"
with open(tauri_conf) as f:
    tauri_version = json.load(f)["version"]
print(f"{pkg_version} {cargo_version} {tauri_version}")
EOF
}

if [[ $# -ne 1 ]]; then
  usage
fi

if [[ "$1" == "--check" ]]; then
  read -r pkg cargo tauri <<<"$(get_versions)"
  if [[ "$pkg" == "$cargo" && "$pkg" == "$tauri" ]]; then
    echo "Versions in sync: $pkg"
    exit 0
  else
    echo "Versions out of sync: package.json=$pkg Cargo.toml=$cargo tauri.conf.json=$tauri" >&2
    exit 1
  fi
fi

NEW_VERSION="$1"
if [[ ! "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Invalid version: $NEW_VERSION (expected X.Y.Z)" >&2
  exit 1
fi

python3 - "$PKG_JSON" "$CARGO_TOML" "$TAURI_CONF" "$NEW_VERSION" <<'EOF'
import json, re, sys
pkg_json, cargo_toml, tauri_conf, new_version = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]

with open(pkg_json) as f:
    pkg = json.load(f)
pkg["version"] = new_version
with open(pkg_json, "w") as f:
    json.dump(pkg, f, indent=2)
    f.write("\n")

with open(cargo_toml) as f:
    content = f.read()
content, n = re.subn(r'^version\s*=\s*"[^"]+"', f'version = "{new_version}"', content, count=1, flags=re.M)
assert n == 1, "version field not found in Cargo.toml"
with open(cargo_toml, "w") as f:
    f.write(content)

with open(tauri_conf) as f:
    conf = json.load(f)
conf["version"] = new_version
with open(tauri_conf, "w") as f:
    json.dump(conf, f, indent=2)
    f.write("\n")
EOF

echo "Bumped version to $NEW_VERSION"
