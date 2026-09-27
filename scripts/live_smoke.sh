#!/usr/bin/env bash
# Live smoke test: a headless run against the live-smoke fixture, asserting its failing
# test passes afterwards. Requires AI_API_KEY. Run per provider: live_smoke.sh <deepseek|qwen>
# Exit codes: 0 pass, 1 fail, 3 the provider rejected the key (it belongs to the other provider).
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
if ! cargo run --quiet --release --locked --manifest-path "$root/Cargo.toml" --bin banana-harness -- \
  run --provider "$provider" --workspace "$work" --json "$issue" >"$events"; then
  if grep -q '"type":"turn.done"' "$events" && grep -qE 'HTTP 40[13]' "$events"; then
    echo "live smoke ($provider): SKIP -- the provider rejected AI_API_KEY"
    exit 3
  fi
  echo "live smoke ($provider): FAIL -- the headless run ended with an error:" >&2
  grep '"type":"turn.done"' "$events" >&2 || cat "$events" >&2
  exit 1
fi

if "$work/tests/test_calc.sh"; then
  echo "live smoke ($provider): PASS -- the agent fixed the fixture"
else
  echo "live smoke ($provider): FAIL -- the fixture's test still fails after the run" >&2
  echo "-- events ($events) --" >&2
  cat "$events" >&2
  exit 1
fi
