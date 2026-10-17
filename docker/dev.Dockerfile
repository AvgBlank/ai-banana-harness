# The development image: the toolchain CI's Linux jobs use, for `make docker-*`. Build it with
# `make docker-image`, which passes the versions from docker/versions.sh (no defaults here, so a
# build without them fails instead of drifting).
ARG RUST_VERSION
ARG NODE_VERSION
FROM node:${NODE_VERSION}-bookworm-slim AS node

FROM rust:${RUST_VERSION}-bookworm
ARG RUST_VERSION
ARG PNPM_VERSION
RUN test -n "$PNPM_VERSION" || { echo "build with make docker-image: PNPM_VERSION is required" >&2; exit 1; }

# A C compiler for SQLite and TLS comes with the base image. Xvfb, procps (the end-to-end tests
# use pgrep) and the libraries Electron loads are added for `make docker-gui-e2e`.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      xvfb xauth procps \
      libnss3 libgtk-3-0 libgbm1 libasound2 libxss1 libxtst6 libatk-bridge2.0-0 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && npm install --global --no-fund --no-audit "pnpm@${PNPM_VERSION}"

# rust-toolchain.toml names the channel "1.95", which rustup treats as its own toolchain next to the
# image's "1.95.0": install it now, with the components it asks for, so no run downloads anything.
RUN rustup toolchain install "${RUST_VERSION}" --profile minimal --component rustfmt --component clippy

# The run user is the host user, who owns nothing here: caches live in a volume at /cache and
# the GUI's dependencies in a volume over gui/node_modules (the host's are for another system).
RUN mkdir -p /cache /work/gui/node_modules && chmod 1777 /cache /work /work/gui /work/gui/node_modules
ENV HOME=/cache/home \
    CARGO_HOME=/cache/cargo \
    CARGO_TARGET_DIR=/cache/target \
    XDG_CACHE_HOME=/cache/xdg \
    npm_config_store_dir=/cache/pnpm-store
WORKDIR /work
CMD ["bash"]
