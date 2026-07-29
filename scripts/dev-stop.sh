#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_dev_common.sh
source "$SCRIPT_DIR/_dev_common.sh"

set +e
stop_app
stop_status=$?
set -e
if [[ $stop_status -eq 0 ]]; then
  echo "Stopped $APP_NAME."
elif [[ $stop_status -eq 1 ]]; then
  echo "$APP_NAME is not running."
else
  echo "Failed to stop $APP_NAME." >&2
  exit 1
fi
