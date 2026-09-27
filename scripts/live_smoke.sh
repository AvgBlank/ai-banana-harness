#!/usr/bin/env bash
# Live smoke test: a headless run against the live-smoke fixture, asserting its failing
# test passes afterwards. Requires AI_API_KEY. Run per provider: live_smoke.sh <deepseek|qwen>
set -euo pipefail

provider="${1:?usage: live_smoke.sh <deepseek|qwen>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$root/fixtures/live-smoke"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cp -r "$fixture"/. "$work"/
git -C "$work" init -q
git -C "$work" -c user.email=smoke@example.com -c user.name=smoke add -A
git -C "$work" -c user.email=smoke@example.com -c user.name=smoke commit -q -m "fixture"

if "$work/tests/test_calc.sh" >/dev/null 2>&1; then
  echo "live smoke ($provider): fixture already passes before the run; fixture is broken" >&2
  exit 1
fi

issue="$(cat "$fixture/ISSUE.md")"
events="$work/events.jsonl"

echo "live smoke ($provider): running the agent..."
cargo run --quiet --release --locked --manifest-path "$root/Cargo.toml" --bin banana-harness -- \
  run --provider "$provider" --workspace "$work" --json "$issue" >"$events"

if "$work/tests/test_calc.sh"; then
  echo "live smoke ($provider): PASS -- the agent fixed the fixture"
else
  echo "live smoke ($provider): FAIL -- the fixture's test still fails after the run" >&2
  echo "-- events ($events) --" >&2
  cat "$events" >&2
  exit 1
fi
