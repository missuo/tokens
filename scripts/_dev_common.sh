#!/usr/bin/env bash
# Shared build helpers for Menu Bar + CLI.
# shellcheck shell=bash

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="TokensMenuBar"
CLI_BIN="$ROOT/cli/target/release/tokens"
LOG_FILE="/tmp/TokensMenuBar.log"

build_cli() {
  echo "Building tokens CLI (release)..."
  cargo build --release --manifest-path "$ROOT/cli/Cargo.toml" -p tokens-cli
  if [[ ! -x "$CLI_BIN" ]]; then
    echo "CLI binary missing after build: $CLI_BIN" >&2
    exit 1
  fi
  echo "CLI ready: $CLI_BIN"
}

build_app() {
  local config="${1:-debug}"
  echo "Building $APP_NAME ($config)..."
  cd "$ROOT"
  if [[ "$config" == "release" ]]; then
    swift build -c release --product "$APP_NAME"
  else
    swift build --product "$APP_NAME"
  fi
}

build_all() {
  local config="${1:-debug}"
  build_cli
  build_app "$config"
}

app_binary() {
  local config="${1:-debug}"
  printf '%s\n' "$ROOT/.build/$config/$APP_NAME"
}

# Match both exact process names and path-style process names like
# ".build/debug/TokensMenuBar" that show up under ps/pgrep.
app_match_pattern() {
  printf '%s\n' "(^|[/ ])${APP_NAME}( |$)"
}

is_running() {
  if command -v pgrep >/dev/null 2>&1; then
    if pgrep -x "$APP_NAME" >/dev/null 2>&1; then
      return 0
    fi
    if pgrep -f "/${APP_NAME}$|/${APP_NAME} " >/dev/null 2>&1; then
      return 0
    fi
  fi
  ps -axo pid=,command= 2>/dev/null | grep -E "$(app_match_pattern)" | grep -v grep >/dev/null
}

running_pids() {
  local pids=""
  if command -v pgrep >/dev/null 2>&1; then
    pids="$(pgrep -x "$APP_NAME" 2>/dev/null || true)"
    if [[ -z "$pids" ]]; then
      pids="$(pgrep -f "/${APP_NAME}$|/${APP_NAME} " 2>/dev/null || true)"
    fi
  fi
  if [[ -z "$pids" ]]; then
    pids="$(ps -axo pid=,command= 2>/dev/null | grep -E "$(app_match_pattern)" | grep -v grep | awk '{print $1}' || true)"
  fi
  printf '%s' "$pids" | tr '\n' ' '
}

stop_app() {
  if ! is_running; then
    return 1
  fi

  local pids
  pids="$(running_pids)"
  if [[ -n "$pids" ]]; then
    # shellcheck disable=SC2086
    kill $pids 2>/dev/null || true
  fi

  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if ! is_running; then
      return 0
    fi
    sleep 0.2
  done

  pids="$(running_pids)"
  if [[ -n "$pids" ]]; then
    # shellcheck disable=SC2086
    kill -9 $pids 2>/dev/null || true
  fi
  sleep 0.2
  if is_running; then
    return 2
  fi
  return 0
}

launch_app() {
  local app_path="$1"
  if [[ ! -x "$app_path" ]]; then
    echo "Built binary not found or not executable: $app_path" >&2
    exit 1
  fi
  if [[ ! -x "$CLI_BIN" ]]; then
    echo "Repo CLI missing: $CLI_BIN" >&2
    echo "Build both components first with: make build" >&2
    exit 1
  fi

  : >"$LOG_FILE"
  # Point the app at this repo's CLI so Homebrew's older tokens is not used.
  env TOKENS_CLI="$CLI_BIN" "$app_path" >>"$LOG_FILE" 2>&1 </dev/null &
  local pid=$!
  disown "$pid" 2>/dev/null || true
  sleep 0.5

  if is_running || kill -0 "$pid" 2>/dev/null; then
    echo "$APP_NAME is running (pid $(running_pids | xargs || echo "$pid"))"
    echo "Using CLI: $CLI_BIN"
    echo "Logs: $LOG_FILE"
    return 0
  fi

  echo "Failed to start $APP_NAME. Recent log output:" >&2
  tail -n 40 "$LOG_FILE" 2>/dev/null || true
  exit 1
}

resolve_config() {
  local config="${1:-debug}"
  case "$config" in
    debug|release) printf '%s\n' "$config" ;;
    *)
      echo "Usage: $0 [debug|release]" >&2
      exit 1
      ;;
  esac
}
