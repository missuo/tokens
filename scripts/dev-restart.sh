#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_dev_common.sh
source "$SCRIPT_DIR/_dev_common.sh"

CONFIG="$(resolve_config "${1:-debug}")"

build_all "$CONFIG"

echo "Stopping existing $APP_NAME processes..."
set +e
stop_app
stop_status=$?
set -e
if [[ $stop_status -eq 0 ]]; then
  echo "Stopped previous instance."
elif [[ $stop_status -eq 1 ]]; then
  echo "No existing instance."
else
  echo "Warning: could not fully stop previous instance." >&2
fi

echo "Starting $APP_NAME ($CONFIG)"
launch_app "$CONFIG"
