#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

for command in bun node npm; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "${command} is required for launcher smoke tests" >&2
    exit 1
  }
done

BUN_BIN="${BUN_BIN:-$(command -v bun)}"
NODE_BIN="${NODE_BIN:-$(command -v node)}"
LDD_BIN="${LDD_BIN:-$(command -v ldd || true)}"

PLATFORM_PACKAGE="$(node --input-type=module <<'NODE'
import { execSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";

function detectLibcKind() {
  if (process.platform !== "linux") return null;
  const report = process.report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) return "gnu";
  if (report?.sharedObjects?.some((entry) => entry.toLowerCase().includes("musl"))) return "musl";
  if (report?.header?.release?.sourceUrl?.toLowerCase().includes("musl")) return "musl";
  try {
    const output = execSync("ldd --version", {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "pipe"],
    }).toLowerCase();
    if (output.includes("musl")) return "musl";
    if (output.includes("glibc") || output.includes("gnu")) return "gnu";
  } catch (error) {
    const output = `${error?.stdout ?? ""}\n${error?.stderr ?? ""}`.toLowerCase();
    if (output.includes("musl")) return "musl";
    if (output.includes("glibc") || output.includes("gnu")) return "gnu";
  }
  const loaderPresent = (prefix) => ["/lib", "/lib64"].some((dir) => {
    try { return readdirSync(dir).some((entry) => entry.startsWith(prefix)); } catch { return false; }
  });
  const gnu = loaderPresent("ld-linux-");
  const musl = loaderPresent("ld-musl-");
  if (gnu !== musl) return musl ? "musl" : "gnu";
  if (gnu && musl) return existsSync("/etc/alpine-release") ? "musl" : "gnu";
  return "gnu";
}

const arch = process.arch;
if (process.platform === "darwin") {
  if (arch === "arm64") console.log("cli-darwin-arm64");
  else if (arch === "x64") console.log("cli-darwin-x64");
  else process.exit(1);
} else if (process.platform === "linux") {
  const libc = detectLibcKind();
  if (arch === "arm64") console.log(libc === "musl" ? "cli-linux-arm64-musl" : "cli-linux-arm64-gnu");
  else if (arch === "x64") console.log(libc === "musl" ? "cli-linux-x64-musl" : "cli-linux-x64-gnu");
  else process.exit(1);
} else {
  process.exit(1);
}
NODE
)"

[[ -n "${PLATFORM_PACKAGE}" ]] || {
  echo "Unsupported platform for launcher smoke tests: $(uname -s) / $(uname -m)" >&2
  exit 1
}

echo "Building CLI dispatcher and native binary..."
bun run --cwd packages/cli build >/dev/null
cargo build --release -p tokscale-cli >/dev/null

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/tokens-launcher-smoke.XXXXXX")"
cleanup() { rm -rf "${TMP_ROOT}"; }
trap cleanup EXIT

CLI_STAGE="${TMP_ROOT}/cli"
PLATFORM_STAGE="${TMP_ROOT}/${PLATFORM_PACKAGE}"
INSTALL_DIR="${TMP_ROOT}/install"
NPM_CACHE="${TMP_ROOT}/npm-cache"
BUN_ONLY_DIR="${TMP_ROOT}/bun-only-path"
NODE_ONLY_DIR="${TMP_ROOT}/node-only-path"
STALE_PATH_DIR="${TMP_ROOT}/stale-path"

cp -R packages/cli "${CLI_STAGE}"
cp -R "packages/${PLATFORM_PACKAGE}" "${PLATFORM_STAGE}"
mkdir -p "${PLATFORM_STAGE}/bin" "${INSTALL_DIR}" "${NPM_CACHE}" "${BUN_ONLY_DIR}" "${NODE_ONLY_DIR}" "${STALE_PATH_DIR}"
cp target/release/tokens "${PLATFORM_STAGE}/bin/tokens"
chmod +x "${CLI_STAGE}/bin.js" "${PLATFORM_STAGE}/bin/tokens"

STALE_MARKER="${TMP_ROOT}/stale-path-executed"
node --input-type=module - "${STALE_PATH_DIR}/tokens" "${STALE_MARKER}" <<'NODE'
import fs from "node:fs";
const [scriptPath, markerPath] = process.argv.slice(2);
fs.writeFileSync(scriptPath, `#!/bin/sh\ntouch "${markerPath}"\necho "tokens 2.0.0"\n`);
fs.chmodSync(scriptPath, 0o755);
NODE

ln -s "${BUN_BIN}" "${BUN_ONLY_DIR}/bun"
ln -s "${NODE_BIN}" "${NODE_ONLY_DIR}/node"
if [[ -n "${LDD_BIN}" ]]; then
  ln -s "${LDD_BIN}" "${BUN_ONLY_DIR}/ldd"
  ln -s "${LDD_BIN}" "${NODE_ONLY_DIR}/ldd"
fi

PLATFORM_TGZ="$(cd "${PLATFORM_STAGE}" && NPM_CONFIG_CACHE="${NPM_CACHE}" npm pack --silent)"
node --input-type=module - "${CLI_STAGE}/package.json" "tokens-${PLATFORM_PACKAGE}" "file:${PLATFORM_STAGE}/${PLATFORM_TGZ}" <<'NODE'
import fs from "node:fs";
const [manifestPath, packageName, packageSpec] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
manifest.optionalDependencies = { [packageName]: packageSpec };
fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
NODE
CLI_TGZ="$(cd "${CLI_STAGE}" && NPM_CONFIG_CACHE="${NPM_CACHE}" npm pack --silent)"

echo "Installing local release tarball with Bun..."
(
  cd "${INSTALL_DIR}"
  env PATH="${BUN_ONLY_DIR}" bun add "${CLI_STAGE}/${CLI_TGZ}" >/dev/null
)

INSTALLED_BIN="${INSTALL_DIR}/node_modules/.bin/tokens"
[[ -e "${INSTALLED_BIN}" ]] || {
  echo "Installed tokens launcher not found at ${INSTALLED_BIN}" >&2
  exit 1
}

echo "Checking source-tree launcher with Node-only PATH..."
env PATH="${NODE_ONLY_DIR}" "${ROOT_DIR}/packages/cli/bin.js" --no-spinner --version >/dev/null

echo "Checking installed launcher via Bun runtime..."
INSTALLED_VERSION_BUN="$(env PATH="${BUN_ONLY_DIR}" bun "${INSTALLED_BIN}" --no-spinner --version)"
[[ "${INSTALLED_VERSION_BUN}" == tokens* ]] || {
  echo "Unexpected Bun launcher output: ${INSTALLED_VERSION_BUN}" >&2
  exit 1
}

echo "Checking installed launcher with Node-only PATH..."
INSTALLED_VERSION_NODE="$(env PATH="${NODE_ONLY_DIR}" "${INSTALLED_BIN}" --no-spinner --version)"
[[ "${INSTALLED_VERSION_NODE}" == tokens* ]] || {
  echo "Unexpected Node-only launcher output: ${INSTALLED_VERSION_NODE}" >&2
  exit 1
}

echo "Checking missing platform binary does not fall back to PATH..."
find "${INSTALL_DIR}/node_modules" -type f -path '*/tokens-cli-*/bin/tokens' -delete
set +e
STALE_OUTPUT="$(env PATH="${STALE_PATH_DIR}:${NODE_ONLY_DIR}" "${INSTALLED_BIN}" --no-spinner --version 2>&1)"
STALE_CODE=$?
set -e
if [[ ${STALE_CODE} -eq 0 || -e "${STALE_MARKER}" ]]; then
  echo "Launcher executed a stale PATH binary: ${STALE_OUTPUT}" >&2
  exit 1
fi
[[ "${STALE_OUTPUT}" == *"tokens binary not found"* ]] || {
  echo "Unexpected missing-binary error output: ${STALE_OUTPUT}" >&2
  exit 1
}

echo "Launcher smoke tests passed."
