#!/usr/bin/env bash
set -euo pipefail

package_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$package_dir/../.." && pwd)"
cd "$package_dir"

swift build -c debug
(cd "$repo_root" && cargo build -p tokscale-cli --bin tokens)

app_dir="$package_dir/.build/TokscaleMenuBar.app"
contents_dir="$app_dir/Contents"
macos_dir="$contents_dir/MacOS"

mkdir -p "$macos_dir"
cp "$package_dir/.build/debug/tokens-menubar" "$macos_dir/tokens-menubar"
cp "$repo_root/target/debug/tokens" "$macos_dir/tokens"
cat >"$macos_dir/tokens-graph-merged" <<'SH'
#!/usr/bin/env bash
set -uo pipefail

case "${1:-}" in
  -h|--help)
    echo "Usage: tokens-graph-merged"
    echo "Emit merged-home graph JSON with the bundled tokens CLI."
    exit 0
    ;;
esac

APP_DIR="$(cd "$(dirname "$0")" && pwd)"
TOKENS="$APP_DIR/tokens"
MERGED="$HOME/.cache/tokscale-merged"

[ -x "$TOKENS" ] || { echo "bundled tokens not found" >&2; exit 1; }
[ -d "$MERGED" ] || { echo "merged home missing" >&2; exit 1; }

for src in .claude/projects .claude/transcripts .codex/sessions .codex/archived_sessions .gemini/tmp .openclaw/agents; do
  [ -d "$HOME/$src" ] || continue
  mkdir -p "$MERGED/$src"
  rsync -a "$HOME/$src/" "$MERGED/$src/" 2>/dev/null
done

exec "$TOKENS" graph --subagents --no-spinner --home "$MERGED"
SH
chmod +x "$macos_dir/tokens-graph-merged"

cat >"$contents_dir/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>tokens-menubar</string>
  <key>CFBundleIdentifier</key>
  <string>ci.tokens.menubar</string>
  <key>CFBundleName</key>
  <string>Tokens Menu Bar</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

echo "$app_dir"
