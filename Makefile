# Tokens Menu Bar development helpers
#
# Default workflow builds BOTH:
#   1) tokens CLI (release)  — required runtime dependency
#   2) TokensMenuBar app
#
# Common targets:
#   make / make all / make build   build CLI + debug app
#   make restart                   rebuild both, stop old app, start new
#   make restart-release           rebuild both with release app binary
#   make start / make stop
#   make run                       build both, then foreground app run
#   make test / make help

.PHONY: all help build build-release build-app build-app-release cli restart restart-release start start-release stop run test prototype-time-range prototype-report-contract

all: build

help:
	@echo "Tokens Menu Bar make targets"
	@echo ""
	@echo "Primary (CLI + app together):"
	@echo "  make / make all / make build   Build CLI + debug Menu Bar"
	@echo "  make build-release             Build CLI + release Menu Bar"
	@echo "  make restart                   Rebuild both, stop old, start new"
	@echo "  make restart-release           Same with release app binary"
	@echo "  make run                       Build both, then foreground run"
	@echo ""
	@echo "Process control:"
	@echo "  make start                     Start existing debug app (builds if needed)"
	@echo "  make start-release             Start existing release app"
	@echo "  make stop                      Stop running TokensMenuBar"
	@echo ""
	@echo "Split / optional:"
	@echo "  make cli                       Build only tokens CLI"
	@echo "  make build-app                 Build only debug Menu Bar"
	@echo "  make build-app-release         Build only release Menu Bar"
	@echo "  make test                      Run Swift tests"
	@echo "  make prototype-time-range      Run throwaway time-range logic prototype"
	@echo "  make prototype-report-contract Run throwaway report/cache contract prototype"
	@echo "  make help                      Show this help"

# Unified builds (default)
build:
	./scripts/dev-build.sh debug

build-release:
	./scripts/dev-build.sh release

# Split builds (escape hatches)
cli:
	cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli

build-app:
	swift build --product TokensMenuBar

build-app-release:
	swift build -c release --product TokensMenuBar

# Dev loop
restart:
	./scripts/dev-restart.sh debug

restart-release:
	./scripts/dev-restart.sh release

start:
	./scripts/dev-start.sh debug

start-release:
	./scripts/dev-start.sh release

stop:
	./scripts/dev-stop.sh

run: build
	@CLI_BIN="$(CURDIR)/cli/target/release/tokens"; \
	if [ ! -x "$$CLI_BIN" ]; then echo "CLI missing: $$CLI_BIN" >&2; exit 1; fi; \
	echo "Using CLI: $$CLI_BIN"; \
	TOKENS_CLI="$$CLI_BIN" swift run TokensMenuBar

test:
	swift test

prototype-time-range:
	cargo run --quiet --manifest-path cli/Cargo.toml -p tokens-cli --example time_range_prototype -- $(ARGS)

prototype-report-contract:
	cargo run --quiet --manifest-path cli/Cargo.toml -p tokens-cli --example report_contract_prototype -- $(ARGS)
