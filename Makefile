##### CONFIG #####
ARGS = $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))

RESET := \033[0m
BOLD  := \033[1m
GREEN := \033[0;32m
CYAN  := \033[0;36m
GRAY  := \033[0;90m

SHELL := bash
.DEFAULT_GOAL := help
MAKEFLAGS += --no-print-directory

##### TARGETS #####
.PHONY: help install fmt lint test deny check build run install-bin wheel sdist clean

help:
	@printf "$(BOLD)$(CYAN)$$(cargo pkgid -p law-cli 2>/dev/null | sed 's|.*/||; s|#.*||')$(RESET) $(GRAY)· cargo · rustfmt · clippy · cargo-deny · prek$(RESET)\n\n"
	@awk 'BEGIN{FS=":.*##"} /^[a-z][a-zA-Z0-9_-]*:.*##/{printf "  $(GREEN)%-12s$(RESET) $(GRAY)%s$(RESET)\n",$$1,$$2}' $(MAKEFILE_LIST)

install: ## toolchain, cargo-deny and the git hooks
	@rustup show active-toolchain >/dev/null
	@command -v cargo-deny >/dev/null || cargo install --locked cargo-deny
	@prek install

fmt: ## format every file
	@cargo fmt --all

lint: ## rustfmt check and clippy with the house lints
	@cargo fmt --all -- --check
	@cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test: ## every test
	@cargo test --workspace --all-targets --all-features --locked $(ARGS)

deny: ## advisories, licenses, bans, sources
	@cargo deny check

check: lint test deny ## the full gate, as CI runs it

build: ## release build
	@cargo build --release --locked

run: ## run the binary: make run -- <args>
	@cargo run --release -p law-cli -- $(ARGS)

install-bin: ## install the binary into ~/.cargo/bin from this checkout
	@cargo install --locked --path crates/law-cli

wheel: ## the wheel, the binary inside, into dist/
	@uvx maturin@1.16.0 build --release --out dist

sdist: ## the source distribution, into dist/
	@uvx maturin@1.16.0 sdist --out dist

clean: ## remove build artifacts
	@cargo clean

%:
	@:
