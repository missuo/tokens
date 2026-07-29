#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_dev_common.sh
source "$SCRIPT_DIR/_dev_common.sh"

CONFIG="$(resolve_config "${1:-debug}")"
APP="$(app_binary "$CONFIG")"

if is_running; then
  echo "$APP_NAME is already running (pid $(running_pids))"
  echo "Use ./scripts/dev-restart.sh (or make restart) to rebuild both CLI + app and relaunch."
  exit 0
fi

if [[ ! -x "$APP" || ! -x "$CLI_BIN" ]]; then
  echo "Missing build artifacts. Building CLI + app first..."
  build_all "$CONFIG"
fi

echo "Starting $APP"
launch_app "$APP"
