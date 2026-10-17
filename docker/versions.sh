#!/bin/sh
# The versions the development image is built with, printed as shell assignments. Rust and pnpm
# come from the files that own them; Node's exact version is pinned here, and must agree with
# gui/.node-version (major) and gui/package.json "engines" (floor). Exits 1 when they disagree.
# Usage: versions.sh [repository root]
NODE_VERSION=24.22.0

root=${1:-$(dirname "$0")/..}
fail() { echo "docker/versions.sh: $1" >&2; exit 1; }

rust=$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' "$root/rust-toolchain.toml")
pnpm=$(sed -n 's/.*"packageManager": *"pnpm@\([^"]*\)".*/\1/p' "$root/gui/package.json")
floor=$(sed -n 's/.*"node": *"^\([0-9.]*\)".*/\1/p' "$root/gui/package.json")
major=$(tr -d ' \n' < "$root/gui/.node-version")

[ -n "$rust" ] || fail "no channel in rust-toolchain.toml"
[ -n "$pnpm" ] || fail "no packageManager pnpm version in gui/package.json"
[ -n "$floor" ] || fail "no engines.node floor in gui/package.json"
[ -n "$major" ] || fail "gui/.node-version is empty"

[ "${NODE_VERSION%%.*}" = "${major%%.*}" ] || fail "Node $NODE_VERSION is not in gui/.node-version's major ($major)"
lowest=$(printf '%s\n%s\n' "$floor" "$NODE_VERSION" | sort -t. -k1,1n -k2,2n -k3,3n | head -n 1)
[ "$lowest" = "$floor" ] || fail "Node $NODE_VERSION is below the engines floor $floor in gui/package.json"

echo "RUST_VERSION=$rust"
echo "NODE_VERSION=$NODE_VERSION"
echo "PNPM_VERSION=$pnpm"
