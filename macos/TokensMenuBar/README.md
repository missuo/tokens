# Tokens Menu Bar

macOS menu bar app that shows local AI coding token usage by calling the `tokens` CLI.

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
