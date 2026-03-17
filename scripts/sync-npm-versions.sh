#!/usr/bin/env bash
# Sync npm package.json versions with the Rust workspace version.
# Called by cargo-release as a pre-release hook.
# Arguments: $1 = old version, $2 = new version
set -euo pipefail

VERSION="${2:-$1}"
ROOT="$(git rev-parse --show-toplevel)"

update_version() {
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('$1', 'utf8'));
    pkg.version = '$VERSION';
    fs.writeFileSync('$1', JSON.stringify(pkg, null, 2) + '\n');
  "
}

update_optional_deps() {
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('$1', 'utf8'));
    pkg.version = '$VERSION';
    if (pkg.optionalDependencies) {
      for (const key of Object.keys(pkg.optionalDependencies)) {
        if (key.startsWith('@fallow-cli/')) {
          pkg.optionalDependencies[key] = '$VERSION';
        }
      }
    }
    fs.writeFileSync('$1', JSON.stringify(pkg, null, 2) + '\n');
  "
}

# Update main fallow package (version + optionalDependencies)
update_optional_deps "$ROOT/npm/fallow/package.json"
echo "  Updated fallow/package.json → $VERSION"

# Update platform-specific npm packages
for pkg in "$ROOT"/npm/*/package.json; do
  case "$pkg" in
    */fallow/package.json) continue ;; # Already handled above
  esac
  [ -f "$pkg" ] || continue
  update_version "$pkg"
done

echo "  Updated all platform package versions → $VERSION"
