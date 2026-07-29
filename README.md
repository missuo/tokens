# Tokens Menu Bar

macOS menu bar app for local AI coding token usage.

This repository is organized around the app first:

- Swift package at the repo root (`Package.swift`, `Sources/`, `Tests/`)
- `cli/` — `tokens-cli` + `tokens-core` runtime dependency
- `docs/` — product design and implementation notes
- `design/` — UI references and prototypes

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
swift run TokensMenuBar
```

## Tests

```bash
swift test
```

## Docs and design

- Design/spec: `docs/design-spec.md`
- Implementation plan: `docs/implementation-plan.md`
- UI references: `design/menubar-ui-v1/`
- Settings scan-interval prototype: `design/settings-scan-interval/`

## Notes

This tree was cleaned and restructured from the original Tokens monorepo fork.
The web leaderboard, npm packaging, and unrelated monorepo tooling were removed.
