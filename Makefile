# Tokens Menu Bar development helpers
#
# Common targets:
#   make restart          rebuild debug binary, stop old app, start new one
#   make restart-release  same with release binary
#   make start            start existing debug binary
#   make stop             stop running TokensMenuBar
#   make run              foreground debug run via swift
#   make test             run Swift package tests
#   make cli              build tokens CLI (release)
#   make help             list targets

.PHONY: help restart restart-release start start-release stop run test build build-release cli

help:
	@echo "Tokens Menu Bar make targets"
	@echo ""
	@echo "  make restart           Rebuild debug app, stop old process, start new"
	@echo "  make restart-release   Rebuild release app, stop old process, start new"
	@echo "  make start             Start existing debug binary"
	@echo "  make start-release     Start existing release binary"
	@echo "  make stop              Stop running TokensMenuBar"
	@echo "  make run               Foreground debug run (swift run)"
	@echo "  make build             Build debug app"
	@echo "  make build-release     Build release app"
	@echo "  make test              Run Swift tests"
	@echo "  make cli               Build tokens CLI (release)"

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

run:
	swift run TokensMenuBar

build:
	swift build --product TokensMenuBar

build-release:
	swift build -c release --product TokensMenuBar

test:
	swift test

cli:
	cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
