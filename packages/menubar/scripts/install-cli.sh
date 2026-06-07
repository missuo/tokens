#!/usr/bin/env bash
set -euo pipefail

# Build the current tokens CLI (with the companion-summary subcommand) and install it
# to ~/.local/bin, which the menu bar app prefers for quota refresh. This avoids both
# the macOS Desktop-access prompt (the repo build lives under ~/Desktop) and the
# outdated Homebrew binary that lacks companion-summary. Re-run after changing the CLI;
# a stale binary here makes the menu bar refresh fail silently and the quota freeze.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

cargo build -p tokscale-cli --bin tokens --release
mkdir -p "$HOME/.local/bin"
cp "target/release/tokens" "$HOME/.local/bin/tokens"

echo "Installed tokens to $HOME/.local/bin/tokens"
"$HOME/.local/bin/tokens" --no-spinner companion-summary --help >/dev/null 2>&1 \
  && echo "companion-summary: OK" \
  || echo "WARNING: companion-summary not available in this binary"
