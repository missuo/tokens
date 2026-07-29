#!/usr/bin/env bash
# Shared build helpers for Menu Bar + CLI.
# shellcheck shell=bash

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="TokensMenuBar"
CLI_BIN="$ROOT/cli/target/release/tokens"
LOG_FILE="/tmp/TokensMenuBar.log"
PID_FILE="/tmp/TokensMenuBar.pid"

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
  printf "%s\n" "$ROOT/.build/$config/$APP_NAME"
}

# Known binary locations for this repo (debug + release + SPM product path).
app_binary_patterns() {
  local config="${1:-debug}"
  local bin real
  bin="$(app_binary "$config")"
  printf "%s\n" "$bin"
  if [[ -e "$bin" ]]; then
    real="$(/usr/bin/python3 -c "import os,sys; print(os.path.realpath(sys.argv[1]))" "$bin" 2>/dev/null || true)"
    if [[ -n "$real" && "$real" != "$bin" ]]; then
      printf "%s\n" "$real"
    fi
  fi
  printf "%s\n" \
    "$ROOT/.build/debug/$APP_NAME" \
    "$ROOT/.build/release/$APP_NAME" \
    "$ROOT/.build/out/Products/Debug/$APP_NAME" \
    "$ROOT/.build/out/Products/Release/$APP_NAME"
}

is_alive_pid() {
  local pid="$1"
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

command_for_pid() {
  local pid="$1"
  /bin/ps -p "$pid" -o command= 2>/dev/null || true
}

# True if a process command line is our Menu Bar binary.
is_app_command() {
  local cmd="$1"
  local pattern

  [[ -z "$cmd" ]] && return 1
  case "$cmd" in
    *"/scripts/"*|*"make "*|*"/bin/zsh "*|*"/bin/bash "*) return 1 ;;
  esac

  while IFS= read -r pattern; do
    [[ -z "$pattern" ]] && continue
    case "$cmd" in
      *"$pattern"*) return 0 ;;
    esac
  done < <(app_binary_patterns)

  return 1
}

running_pids() {
  local pids=()
  local pid cmd
  local seen_line

  if [[ -f "$PID_FILE" ]]; then
    pid="$(/usr/bin/tr -d "[:space:]" <"$PID_FILE" 2>/dev/null || true)"
    if is_alive_pid "$pid"; then
      cmd="$(command_for_pid "$pid")"
      if is_app_command "$cmd"; then
        pids+=("$pid")
      fi
    fi
  fi

  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    # Keep first token as pid; remainder is command.
    pid="${line%% *}"
    cmd="${line#* }"
    if is_app_command "$cmd"; then
      pids+=("$pid")
    fi
  done < <(/bin/ps -axo pid=,command= 2>/dev/null | /usr/bin/grep -F "$APP_NAME" || true)

  if ((${#pids[@]} == 0)); then
    return 0
  fi

  printf "%s\n" "${pids[@]}" | /usr/bin/awk "NF && !seen[\$0]++" | /usr/bin/tr "\n" " "
}

is_running() {
  local pids
  pids="$(running_pids)"
  [[ -n "${pids// /}" ]]
}

stop_app() {
  if ! is_running; then
    rm -f "$PID_FILE"
    return 1
  fi

  local pids
  pids="$(running_pids)"
  if [[ -n "${pids// /}" ]]; then
    # shellcheck disable=SC2086
    kill $pids 2>/dev/null || true
  fi

  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    if ! is_running; then
      rm -f "$PID_FILE"
      return 0
    fi
    sleep 0.2
  done

  pids="$(running_pids)"
  if [[ -n "${pids// /}" ]]; then
    # shellcheck disable=SC2086
    kill -9 $pids 2>/dev/null || true
  fi
  sleep 0.2
  if is_running; then
    return 2
  fi
  rm -f "$PID_FILE"
  return 0
}

launch_app() {
  local config="${1:-debug}"
  local bin
  local pid

  bin="$(app_binary "$config")"
  if [[ ! -x "$bin" ]]; then
    echo "Built binary not found or not executable: $bin" >&2
    exit 1
  fi
  if [[ ! -x "$CLI_BIN" ]]; then
    echo "Repo CLI missing: $CLI_BIN" >&2
    echo "Build both components first with: make build" >&2
    exit 1
  fi

  : >"$LOG_FILE"
  rm -f "$PID_FILE"

  # Direct binary launch is the reliable local path.
  # A minimal .app + LaunchServices open currently aborts during AppKit
  # registration on this machine, so we do not use that path for development.
  env TOKENS_CLI="$CLI_BIN" "$bin" >>"$LOG_FILE" 2>&1 </dev/null &
  pid=$!
  echo "$pid" >"$PID_FILE"
  disown "$pid" 2>/dev/null || true

  # Fail fast if the process dies immediately.
  local i
  for i in 1 2 3 4 5; do
    sleep 0.2
    if ! is_alive_pid "$pid"; then
      echo "Failed to start $APP_NAME (exited immediately). Recent log output:" >&2
      /usr/bin/tail -n 40 "$LOG_FILE" 2>/dev/null || true
      rm -f "$PID_FILE"
      exit 1
    fi
  done

  if ! is_app_command "$(command_for_pid "$pid")"; then
    echo "Started pid $pid but process command does not look like $APP_NAME." >&2
    echo "Command: $(command_for_pid "$pid")" >&2
    exit 1
  fi

  echo "$APP_NAME is running (pid $pid)"
  echo "App binary: $bin"
  echo "Using CLI: $CLI_BIN"
  echo "Logs: $LOG_FILE"
  echo "Look for the status item in the top menu bar (no Dock icon)."
}

resolve_config() {
  local config="${1:-debug}"
  case "$config" in
    debug|release) printf "%s\n" "$config" ;;
    *)
      echo "Usage: $0 [debug|release]" >&2
      exit 1
      ;;
  esac
}
