#!/usr/bin/env bash
#
# One-click installer for the `tokens` CLI on Linux.
#
# Downloads the prebuilt release binary for your architecture, installs it, and
# sets up a systemd *user* timer that runs `tokens --no-spinner submit` every
# 30 minutes. The CLI exits between runs instead of staying resident.
#
#   curl -fsSL https://raw.githubusercontent.com/missuo/tokens/main/install.sh | bash
#
# Optional environment overrides:
#   TOKENS_VERSION=v3.0.0       install a specific release (default: latest)
#   TOKENS_INSTALL_DIR=<dir>    install location (default: /usr/local/bin as
#                               root, otherwise ~/.local/bin)
#   TOKENS_NO_SERVICE=1         skip the systemd service setup
#
set -euo pipefail

REPO="missuo/tokens"
BIN="tokens"

# --- pretty output --------------------------------------------------------
if [ -t 1 ]; then
  BOLD=$'\033[1m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'; BLUE=$'\033[0;34m'; RED=$'\033[0;31m'; NC=$'\033[0m'
else
  BOLD=""; GREEN=""; YELLOW=""; BLUE=""; RED=""; NC=""
fi
info() { printf "%s %s\n" "${BLUE}==>${NC}" "$*"; }
ok()   { printf "%s %s\n" "${GREEN}✓${NC}" "$*"; }
warn() { printf "%s %s\n" "${YELLOW}!${NC}" "$*"; }
die()  { printf "%s %s\n" "${RED}✗${NC}" "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- platform detection ---------------------------------------------------
[ "$(uname -s)" = "Linux" ] || die "This installer is Linux-only. On macOS: brew install owo-network/brew/tokens"

case "$(uname -m)" in
  x86_64|amd64)  TARGET="x86_64-unknown-linux-gnu" ;;
  aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
  *) die "Unsupported architecture: $(uname -m)" ;;
esac

if have curl; then
  fetch() { curl -fsSL "$1"; }
  download() { curl -fsSL -o "$2" "$1"; }
elif have wget; then
  fetch() { wget -qO- "$1"; }
  download() { wget -qO "$2" "$1"; }
else
  die "Need either curl or wget installed."
fi

# --- resolve version ------------------------------------------------------
VERSION="${TOKENS_VERSION:-}"
if [ -z "$VERSION" ]; then
  info "Resolving latest release of $REPO ..."
  VERSION="$(fetch "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
  [ -n "$VERSION" ] || die "Could not resolve the latest version. Set TOKENS_VERSION=vX.Y.Z and retry."
fi
ok "Installing $BIN $VERSION ($TARGET)"

ARCHIVE="${BIN}-${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/${VERSION}/${ARCHIVE}"

# --- download + verify ----------------------------------------------------
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
info "Downloading $ARCHIVE ..."
download "$URL" "$TMP/$ARCHIVE" || die "Download failed: $URL"

if fetch "${URL}.sha256" > "$TMP/sum" 2>/dev/null && [ -s "$TMP/sum" ]; then
  expected="$(cut -d ' ' -f1 < "$TMP/sum")"
  if have sha256sum; then actual="$(sha256sum "$TMP/$ARCHIVE" | cut -d ' ' -f1)"
  elif have shasum;  then actual="$(shasum -a 256 "$TMP/$ARCHIVE" | cut -d ' ' -f1)"
  else actual=""; fi
  if [ -n "$actual" ]; then
    [ "$expected" = "$actual" ] || die "Checksum mismatch (expected $expected, got $actual)"
    ok "Checksum verified"
  fi
fi

tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
[ -f "$TMP/$BIN" ] || die "Binary '$BIN' not found inside the archive."
chmod +x "$TMP/$BIN"

# --- install --------------------------------------------------------------
if [ -n "${TOKENS_INSTALL_DIR:-}" ]; then
  INSTALL_DIR="$TOKENS_INSTALL_DIR"
elif [ "$(id -u)" = "0" ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="$HOME/.local/bin"
fi
mkdir -p "$INSTALL_DIR"
install -m 0755 "$TMP/$BIN" "$INSTALL_DIR/$BIN"
BIN_PATH="$INSTALL_DIR/$BIN"
ok "Installed to $BIN_PATH"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) warn "$INSTALL_DIR is not on your PATH. Add this to your shell profile:"
     printf "      %sexport PATH=\"%s:\$PATH\"%s\n" "$GREEN" "$INSTALL_DIR" "$NC" ;;
esac

# --- systemd user service + timer ------------------------------------------
SERVICE_READY=0
if [ "${TOKENS_NO_SERVICE:-}" != "1" ] && have systemctl; then
  UNIT_DIR="$HOME/.config/systemd/user"
  mkdir -p "$UNIT_DIR"
  cat > "$UNIT_DIR/tokens.service" <<EOF
[Unit]
Description=tokens - AI usage submitter
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=$BIN_PATH --no-spinner submit

[Install]
WantedBy=default.target
EOF
  cat > "$UNIT_DIR/tokens.timer" <<EOF
[Unit]
Description=tokens - scheduled AI usage submitter

[Timer]
OnActiveSec=2min
OnUnitActiveSec=30min
Persistent=true
Unit=tokens.service

[Install]
WantedBy=timers.target
EOF
  systemctl --user daemon-reload 2>/dev/null || true
  ok "Installed systemd user service + timer: $UNIT_DIR/tokens.service, $UNIT_DIR/tokens.timer"
  SERVICE_READY=1
else
  [ "${TOKENS_NO_SERVICE:-}" = "1" ] || warn "systemctl not found — skipping background service setup."
fi

# --- next steps -----------------------------------------------------------
printf "\n%sInstalled. Next steps:%s\n\n" "$BOLD" "$NC"
printf "  %s1.%s Authenticate (one-time):\n       %stokens login%s\n\n" "$BOLD" "$NC" "$GREEN" "$NC"
if [ "$SERVICE_READY" = "1" ]; then
  printf "  %s2.%s Schedule background submits:\n       %ssystemctl --user enable --now tokens.timer%s\n\n" "$BOLD" "$NC" "$GREEN" "$NC"
  printf "  %s3.%s Start it automatically on boot without an active login:\n       %ssudo loginctl enable-linger \"\$USER\"%s\n\n" "$BOLD" "$NC" "$GREEN" "$NC"
  printf "  Logs:   %sjournalctl --user -u tokens -f%s\n" "$BLUE" "$NC"
  printf "  Status: %ssystemctl --user status tokens.timer%s\n" "$BLUE" "$NC"
else
  printf "  %s2.%s Run it in the background with your own service manager:\n       %stokens serve%s\n" "$BOLD" "$NC" "$GREEN" "$NC"
fi
