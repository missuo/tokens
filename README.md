# Tokens Menu Bar

macOS menu bar app for local AI coding token usage.

This repository keeps only:

- `macos/TokensMenuBar` — Swift menu bar app
- `cli/` — `tokens-cli` + `tokens-core` used by the app
- menubar-related docs and design references

## Requirements

- macOS 13+
- Rust toolchain (to build the CLI)
- Swift 5.9+

## Build the CLI

```bash
cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
```

The app looks for a `tokens` binary that supports:

```bash
tokens usage --json --period <today|7d|30d|all> [--refresh|--force-rescan]
```

Common install locations:

- `~/.local/bin/tokens`
- `/opt/homebrew/bin/tokens`
- `cli/target/release/tokens` during local development

## Run the Menu Bar app

```bash
cd macos/TokensMenuBar
swift run TokensMenuBar
```

## Tests

```bash
cd macos/TokensMenuBar
swift test
```

## Docs and designs

- Design/spec: `docs/superpowers/specs/2026-07-26-macos-menubar-usage-design.md`
- Implementation plan: `docs/superpowers/plans/2026-07-26-menubar-minimal-mono-ui.md`
- UI references: `designs/menubar-ui-v1/`
- Package-local design notes: `macos/TokensMenuBar/designs/`

## Notes

This tree was cleaned down from the original Tokens monorepo fork.
The web leaderboard, npm packaging, and unrelated monorepo tooling were removed.
