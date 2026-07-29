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

## One command

CLI and Menu Bar are a pair. Default commands build **both**:

```bash
make                # build CLI + debug Menu Bar
make restart        # rebuild both, stop old app, start new app
make run            # build both, then run Menu Bar in the foreground
```

`make restart` is the normal development loop.

## Dev workflow

Day-to-day development goes through `make`.

`Makefile` is the stable entrypoint.
Shell scripts under `scripts/` implement the multi-step logic
(`build CLI` / `build app` / `stop old process` / `start new process`).

```bash
make                    # same as make build
make build              # CLI (release) + Menu Bar (debug)
make build-release      # CLI (release) + Menu Bar (release)
make restart            # rebuild both + relaunch
make restart-release
make stop
make start              # start existing build (builds both if missing)
make run                # build both + foreground app
make test
make help
```

Split targets still exist if you need them:

```bash
make cli                # only tokens CLI
make build-app          # only Menu Bar (debug)
make build-app-release  # only Menu Bar (release)
```

Logs go to `/tmp/TokensMenuBar.log`.

When launching via `make restart` / `make start` / `make run`, the app is pointed at this repo’s freshly built CLI:

```text
cli/target/release/tokens
```

That avoids accidentally using an older Homebrew `tokens` that does not support `usage --period`.

## Manual commands

If you prefer not to use Make:

```bash
# build both
./scripts/dev-build.sh
./scripts/dev-build.sh release

# rebuild both + relaunch
./scripts/dev-restart.sh
./scripts/dev-restart.sh release

./scripts/dev-stop.sh
./scripts/dev-start.sh
```

Or the raw toolchains:

```bash
cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
swift build --product TokensMenuBar
TOKENS_CLI="$PWD/cli/target/release/tokens" swift run TokensMenuBar
```

The app looks for a `tokens` binary that supports:

```bash
tokens usage --json --period <today|7d|30d|all> [--refresh|--force-rescan]
```

Resolution order (simplified):

1. `TOKENS_CLI` / settings override
2. repo-local `cli/target/release/tokens`
3. `~/.local/bin/tokens`
4. Homebrew / PATH tokens that support `usage --period`

## Tests

```bash
make test
```

## Docs and design

- Design/spec: `docs/design-spec.md`
- Implementation plan: `docs/implementation-plan.md`
- UI references: `design/menubar-ui-v1/`
- Settings scan-interval prototype: `design/settings-scan-interval/`

## Notes

This tree was cleaned and restructured from the original Tokens monorepo fork.
The web leaderboard, npm packaging, and unrelated monorepo tooling were removed.

## Acknowledgments

This project is based on a fork of [missuo/tokens](https://github.com/missuo/tokens).

Thanks to the original Tokens project and its maintainers for the CLI, usage scanning, and open-source foundation that this Menu Bar app builds on.
