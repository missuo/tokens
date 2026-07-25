#!/usr/bin/env bash
#
# Write one version into every manifest that carries it.
#
# There is no separate coherence check because there is nothing to check: the
# Cargo workspace, the launcher and all eight platform packages are written from
# the same argument, in one pass. The launcher pins its platform packages by
# exact version, so a drift here would publish an uninstallable release.
set -euo pipefail

VERSION="${1:?usage: set-version.sh <version>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

# Rust workspace.
perl -0pi -e "s/^(\[workspace\.package\](?:.|\n)*?^version = )\"[^\"]+\"/\${1}\"${VERSION}\"/m" cli/Cargo.toml
grep -q "version = \"${VERSION}\"" cli/Cargo.toml || { echo "ERROR: cli/Cargo.toml not updated" >&2; exit 1; }

# Keep Cargo.lock in step so the build does not rewrite it mid-release.
cargo update --manifest-path cli/Cargo.toml --workspace --offline >/dev/null 2>&1 || \
  cargo update --manifest-path cli/Cargo.toml --workspace >/dev/null

# Launcher and platform packages.
node - "${VERSION}" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");
const version = process.argv[2];

const dirs = fs.readdirSync("packages").filter((d) => d.startsWith("cli"));
for (const dir of dirs) {
  const file = path.join("packages", dir, "package.json");
  if (!fs.existsSync(file)) continue;
  const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
  pkg.version = version;
  // The launcher resolves its binary through optionalDependencies pinned to an
  // exact version; they move together or the install picks nothing.
  if (pkg.optionalDependencies) {
    for (const name of Object.keys(pkg.optionalDependencies)) {
      if (name.startsWith("tokens-cli-")) pkg.optionalDependencies[name] = version;
    }
  }
  fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
  console.log(`  ${file} -> ${version}`);
}
NODE

echo "set-version: ${VERSION}"
