# Evaluator interface: make setup, make run, make test, make clean.
# AI_API_KEY and WORKSPACE are passed to the binary through the environment.

# Prefer rustup's cargo (which honours rust-toolchain.toml), then any cargo on PATH.
# Recursively expanded, so it sees a rustup that `install-rust` has just installed.
CARGO ?= $(if $(wildcard $(HOME)/.cargo/bin/cargo),$(HOME)/.cargo/bin/cargo,cargo)
BIN := target/release/banana-harness

.PHONY: setup run test lint clean check-cc install-rust

setup: check-cc install-rust
	$(CARGO) build --release --locked

run:
	@test -x $(BIN) || { echo "error: $(BIN) not found; run 'make setup' first." >&2; exit 1; }
	@$(BIN)

test:
	$(CARGO) test --workspace --locked
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

clean:
	$(CARGO) clean

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
