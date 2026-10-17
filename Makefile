# Evaluator interface: make setup, make run, make test, make clean.
# AI_API_KEY and WORKSPACE are passed to the binary through the environment.

# Prefer rustup's cargo (which honours rust-toolchain.toml), then any cargo on PATH.
# Recursively expanded, so it sees a rustup that `install-rust` has just installed.
CARGO ?= $(if $(wildcard $(HOME)/.cargo/bin/cargo),$(HOME)/.cargo/bin/cargo,cargo)
BIN := target/release/banana-harness
GUI := gui
PNPM ?= pnpm
TARGET_DIR := $(or $(CARGO_TARGET_DIR),target)
SIDECAR_DEBUG := $(abspath $(TARGET_DIR)/debug/banana-harness)
# The development container (docker/dev.Dockerfile): the checkout is mounted, the caches and the
# GUI's dependencies live in named volumes, and the user is the host user. No key is passed unless
# PASS_API_KEY=1 is set for that run. Cargo gets DEV_JOBS parallel jobs (not one per core), because
# Docker Desktop's VM has far less memory than the Mac: parallel linking gets killed in it and the
# GUI tests (VITEST_MAX_WORKERS) time out when every core runs one.
DOCKER ?= docker
DEV_JOBS ?= 2
DEV_IMAGE ?= banana-harness-dev
DEV_RUN = $(DOCKER) run --rm $(if $(DEV_TTY),-it,) --user "$$(id -u):$$(id -g)" \
  -v "$(CURDIR)":/work -v banana-dev-cache:/cache -v banana-dev-gui-node-modules:/work/gui/node_modules \
  -e CARGO_BUILD_JOBS=$(DEV_JOBS) -e VITEST_MAX_WORKERS=$(DEV_JOBS) $(if $(PASS_API_KEY),-e AI_API_KEY,) $(DEV_ENV) $(DEV_IMAGE)
SIDECAR_RELEASE := $(abspath $(TARGET_DIR)/release/banana-harness)
FAKE_PROVIDER_DEBUG := $(abspath $(TARGET_DIR)/debug/fake-provider)

.PHONY: setup run test lint clean check-cc install-rust \
        gui-setup gui-sidecar gui-dev gui-build gui-lint gui-test gui-e2e gui-leak-scan gui-package gui-package-smoke gui-clean \
        docker-image docker-lint docker-test docker-gui-e2e docker-shell docker-clean gui-maybe-lint gui-maybe-test

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

# The end-to-end suite: Electron against the real sidecar and the fake provider. Not part of `make test`
# (it needs the Electron binary and a display); Linux without a display uses xvfb-run.
gui-e2e: gui-sidecar
	$(CARGO) build --locked -p harness-testkit --bin fake-provider
	$(PNPM) --dir $(GUI) build
	BANANA_HARNESS_BIN=$(SIDECAR_DEBUG) FAKE_PROVIDER_BIN=$(FAKE_PROVIDER_DEBUG) \
	  $(if $(shell command -v xvfb-run),$(if $(DISPLAY),,xvfb-run -a),) $(PNPM) --dir $(GUI) test:e2e

# Fails when the fixed test key is in the test results; run after gui-e2e, before uploading them.
gui-leak-scan:
	node $(GUI)/scripts/scan-test-results.mjs $(GUI)/test-results

# The unsigned macOS app for this Mac's architecture (arm64 only): the release sidecar is staged in
# the bundle (built with this machine's paths remapped out of it), then the DMG and ZIP in
# gui/dist/ are checked.
gui-package:
	@[ "$$(uname -s)" = Darwin ] || { echo "error: gui-package builds the macOS app and only runs on macOS." >&2; exit 1; }
	@[ "$$(uname -m)" = arm64 ] || { echo "error: gui-package builds arm64 only; this Mac is $$(uname -m)." >&2; exit 1; }
	RUSTFLAGS="--remap-path-prefix=$(CURDIR)=. --remap-path-prefix=$(HOME)=~" $(CARGO) build --release --locked --bin banana-harness
	node $(GUI)/scripts/copy-sidecar.mjs $(SIDECAR_RELEASE)
	$(PNPM) --dir $(GUI) build
	$(PNPM) --dir $(GUI) exec electron-builder --mac dmg zip --arm64 --publish never
	node $(GUI)/scripts/verify-package.mjs

# Launches the app made by gui-package through Playwright: its window and a ready sidecar.
gui-package-smoke:
	@[ "$$(uname -s)" = Darwin ] || { echo "error: gui-package-smoke runs the macOS app and only runs on macOS." >&2; exit 1; }
	$(PNPM) --dir $(GUI) exec playwright test -c playwright.packaged.config.ts

docker-preflight:
	@command -v $(DOCKER) >/dev/null || { echo "error: $(DOCKER) is not installed; install Docker to use the docker-* targets." >&2; exit 1; }
	@$(DOCKER) info >/dev/null 2>&1 || { echo "error: the Docker daemon is not running; start Docker Desktop (or the docker service) and try again." >&2; exit 1; }

# Builds the image with the versions docker/versions.sh derives from the files that own them.
docker-image: docker-preflight
	@eval "$$(sh docker/versions.sh)" && $(DOCKER) build -f docker/dev.Dockerfile -t $(DEV_IMAGE) \
	  --build-arg RUST_VERSION="$$RUST_VERSION" --build-arg NODE_VERSION="$$NODE_VERSION" \
	  --build-arg PNPM_VERSION="$$PNPM_VERSION" docker

docker-lint: docker-image
	$(DEV_RUN) sh -c 'make gui-setup && make lint'

docker-test: docker-image
	$(DEV_RUN) sh -c 'make gui-setup && make test'

# The end-to-end suite under Xvfb; Chromium's sandbox cannot start in a container, so CI=1 adds --no-sandbox.
docker-gui-e2e: docker-image
	$(MAKE) DEV_ENV="-e CI=1" _docker-gui-e2e

_docker-gui-e2e:
	$(DEV_RUN) sh -c 'make gui-setup && make gui-e2e'

docker-shell: docker-image
	$(MAKE) DEV_TTY=1 _docker-shell

_docker-shell:
	$(DEV_RUN) bash

# Removes the image and the volumes (the next run downloads and builds everything again).
docker-clean: docker-preflight
	-$(DOCKER) volume rm banana-dev-cache banana-dev-gui-node-modules
	-$(DOCKER) image rm $(DEV_IMAGE)

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
