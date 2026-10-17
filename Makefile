# Evaluator interface: make setup, make run, make test, make clean.
# AI_API_KEY and WORKSPACE are passed to the binary through the environment.

# Prefer rustup's cargo (which honours rust-toolchain.toml), then any cargo on PATH.
# Recursively expanded, so it sees a rustup that `install-rust` has just installed.
CARGO ?= $(if $(wildcard $(HOME)/.cargo/bin/cargo),$(HOME)/.cargo/bin/cargo,cargo)
BIN := target/release/banana-harness
GUI := gui
PNPM ?= pnpm
SIDECAR_DEBUG := $(abspath target/debug/banana-harness)

.PHONY: setup run test lint clean check-cc install-rust \
        gui-setup gui-sidecar gui-dev gui-build gui-lint gui-test gui-clean gui-maybe-lint gui-maybe-test

setup: check-cc install-rust
	$(CARGO) build --release --locked

run:
	@test -x $(BIN) || { echo "error: $(BIN) not found; run 'make setup' first." >&2; exit 1; }
	@$(BIN)

# Loopback must bypass any proxy from the environment: the offline suite talks to a fake provider
# on 127.0.0.1. The user's own exclusions are kept.
TEST_ENV = NO_PROXY="$${NO_PROXY:+$$NO_PROXY,}127.0.0.1,localhost" no_proxy="$${no_proxy:+$$no_proxy,}127.0.0.1,localhost"

test:
	$(TEST_ENV) $(CARGO) test --workspace --locked
	@$(MAKE) --no-print-directory gui-maybe-test
	@# One key belongs to one provider: the other one's 401 is a skip, not a failure.
	@if [ -z "$$AI_API_KEY" ]; then echo "AI_API_KEY not set; skipping the live smoke test."; exit 0; fi; \
	passed=0; \
	for provider in deepseek qwen; do \
	  scripts/live_smoke.sh $$provider; status=$$?; \
	  if [ $$status -eq 0 ]; then passed=$$((passed + 1)); elif [ $$status -ne 3 ]; then exit $$status; fi; \
	done; \
	if [ $$passed -eq 0 ]; then echo "error: both DeepSeek and Qwen rejected AI_API_KEY." >&2; exit 1; fi

lint:
	$(CARGO) fmt --check
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	@$(MAKE) --no-print-directory gui-maybe-lint

clean: gui-clean
	$(CARGO) clean

# The desktop app's checks run inside `make lint` and `make test` only when its dependencies are
# installed (`make gui-setup`) or REQUIRE_GUI=1 (CI); a Rust-only machine gets one line and exit 0.
gui-maybe-lint:
	@if [ -d $(GUI)/node_modules ] || [ "$(REQUIRE_GUI)" = 1 ]; then $(MAKE) --no-print-directory gui-lint; \
	else echo "GUI checks skipped: run 'make gui-setup' (Node 24 and pnpm) to enable them."; fi

gui-maybe-test:
	@if [ -d $(GUI)/node_modules ] || [ "$(REQUIRE_GUI)" = 1 ]; then $(MAKE) --no-print-directory gui-test; \
	else echo "GUI checks skipped: run 'make gui-setup' (Node 24 and pnpm) to enable them."; fi

gui-setup:
	@node -e 'process.exit(+process.versions.node.split(".")[0] === 24 ? 0 : 1)' || { echo "error: the desktop app needs Node 24 (found $$(node -v)); for example 'nvm install 24 && nvm use 24'." >&2; exit 1; }
	@command -v $(PNPM) >/dev/null || { echo "error: pnpm is required (npm i -g pnpm)." >&2; exit 1; }
	$(PNPM) --dir $(GUI) install --frozen-lockfile
	$(PNPM) --dir $(GUI) exec install-electron

# The debug sidecar the app launches in development.
gui-sidecar:
	$(CARGO) build --locked --bin banana-harness

# AI_API_KEY is inherited from the shell, as for `make run`.
gui-dev: gui-sidecar
	BANANA_HARNESS_BIN=$(SIDECAR_DEBUG) $(PNPM) --dir $(GUI) dev

gui-build:
	$(PNPM) --dir $(GUI) build

gui-lint:
	$(PNPM) --dir $(GUI) lint

gui-test:
	$(PNPM) --dir $(GUI) test

gui-clean:
	rm -rf $(GUI)/out $(GUI)/dist $(GUI)/build/sidecar $(GUI)/test-results $(GUI)/playwright-report

check-cc:
	@cc --version >/dev/null 2>&1 || { \
	  echo "error: no C compiler found; it is needed to build SQLite and TLS." >&2; \
	  if [ "$$(uname -s)" = Darwin ]; then \
	    echo "fix: xcode-select --install" >&2; \
	  else \
	    echo "fix: sudo apt install build-essential   (Debian/Ubuntu; elsewhere install gcc)" >&2; \
	  fi; \
	  exit 1; }

install-rust:
	@if ! command -v cargo >/dev/null 2>&1 && [ ! -x "$(HOME)/.cargo/bin/cargo" ]; then \
	  command -v curl >/dev/null 2>&1 || { echo "error: curl is needed to install Rust. fix: sudo apt install curl" >&2; exit 1; }; \
	  echo "Rust not found; installing it with rustup..."; \
	  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal; \
	fi
