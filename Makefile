# Evaluator interface: make setup, make run, make test, make clean.
# AI_API_KEY and WORKSPACE are passed to the binary through the environment.

# Prefer rustup's cargo (which honours rust/rust-toolchain.toml), then any cargo on PATH.
# Recursively expanded, so it sees a rustup that `install-rust` has just installed.
CARGO ?= $(if $(wildcard $(HOME)/.cargo/bin/cargo),$(HOME)/.cargo/bin/cargo,cargo)
BIN := rust/target/release/banana-harness

.PHONY: setup run test lint clean check-cc install-rust

setup: check-cc install-rust
	cd rust && $(CARGO) build --release --locked

run:
	@test -x $(BIN) || { echo "error: $(BIN) not found; run 'make setup' first." >&2; exit 1; }
	@$(BIN)

test:
	cd rust && $(CARGO) test --workspace --locked
	@if [ -n "$$AI_API_KEY" ]; then \
	  rust/scripts/live_smoke.sh deepseek && rust/scripts/live_smoke.sh qwen; \
	else \
	  echo "AI_API_KEY not set; skipping the live smoke test."; \
	fi

lint:
	cd rust && $(CARGO) fmt --check
	cd rust && $(CARGO) clippy --workspace --all-targets --locked -- -D warnings

clean:
	cd rust && $(CARGO) clean

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
