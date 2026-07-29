#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_dev_common.sh
source "$SCRIPT_DIR/_dev_common.sh"

CONFIG="$(resolve_config "${1:-debug}")"
build_all "$CONFIG"
echo "Build complete."
echo "  CLI: $CLI_BIN"
echo "  App: $(app_binary "$CONFIG")"
