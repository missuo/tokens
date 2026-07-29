#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_dev_common.sh
source "$SCRIPT_DIR/_dev_common.sh"

CONFIG="$(resolve_config "${1:-debug}")"

if is_running; then
  echo "$APP_NAME is already running (pid $(running_pids))"
  echo "Use make restart to rebuild CLI + app and relaunch."
  exit 0
fi

if [[ ! -x "$(app_binary "$CONFIG")" || ! -x "$CLI_BIN" ]]; then
  echo "Missing build artifacts. Building CLI + app first..."
  build_all "$CONFIG"
fi

echo "Starting $APP_NAME ($CONFIG)"
launch_app "$CONFIG"
