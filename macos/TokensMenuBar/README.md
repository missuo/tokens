# Tokens Menu Bar

macOS menu bar app that shows local AI coding token usage by calling the `tokens` CLI.

## UI: Minimal Mono v2

Popover + Settings follow **FINAL · 06 Minimal Mono v2** (monospaced Swiss receipt type, spacing-only sections, 4-up breakdown cards, 14-day cost chart with hover, nested list edge fades). Design source and interaction frames:

- `designs/menubar-ui-v1/` (see that folder’s README; shots `full-final.png`, `full-ix-*.png`)

## Requirements

- macOS 13+
- A `tokens` binary on your PATH (or Homebrew / `~/.local/bin`)

Build the CLI from this monorepo:

```bash
cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
# optional: install or symlink into PATH
```

## Run (dev)

```bash
cd macos/TokensMenuBar
swift run TokensMenuBar
```

## Tests

```bash
cd macos/TokensMenuBar
swift test
```

## Behavior

- Scans once on launch via `tokens usage --json --period <p> --refresh`
- Default auto-refresh every 12 hours (configurable)
- Period switches reuse the CLI Layer B snapshot when possible
- Settings → **Full Rescan Now** runs `tokens usage --force-rescan`

See `docs/superpowers/specs/2026-07-26-macos-menubar-usage-design.md`.
