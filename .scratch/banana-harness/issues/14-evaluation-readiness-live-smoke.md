# 14: Evaluation readiness: live smoke test and README

**What to build:**

**Fixture:** a small repository with one failing test and an issue text describing the bug.

**Live smoke script:** runs a headless run (JSON lines) against the fixture with the chosen provider and asserts the test passes afterwards. `make test` runs it only when `AI_API_KEY` is set.

**Headless run:** JSON-lines output and exit codes (0 done, 1 error, 2 cancelled or limit reached) are finalised.

**README:** the evaluator steps (export key, `make setup`, `make run`, `WORKSPACE`), the config layers, the key bindings and the guardrails.

**Blocked by:** 04

**Status:** done, except the real-key smoke pass (unverified: the supplied `AI_API_KEY` was rejected by both providers)

- [ ] With a real key, `make test` fixes the fixture's failing test using DeepSeek, and again using Qwen. **Unverified** — see Notes.
- [x] Without a key, `make test` skips the live smoke and passes.
- [x] Headless JSON output is one valid JSON event per line, and the exit codes match the outcome.
- [x] The README contains no credentials and matches actual behaviour.

## Notes

For whoever runs the live smoke test next, and for 15/16.

**Fixture (`rust/fixtures/live-smoke/`):** deliberately not a Cargo project, so `cargo test
--workspace` never touches it and no extra toolchain (Node/Python) is required. `ISSUE.md` is
the issue text; `src/calc.sh` defines `add()` with an off-by-one bug (`+ $2 - 1`); `tests/
test_calc.sh` sources it and asserts `add 2 3 == 5`. All three are checked in with the shell
scripts' executable bit set.

**Live smoke script (`rust/scripts/live_smoke.sh <deepseek|qwen>`):** copies the fixture into a
fresh `mktemp -d`, `git init`s and commits it there (the agent's guardrails/checkpoint code
expects a git workspace), asserts the copy's test fails *before* the run (a canary against a
broken fixture), builds and runs `cargo run --release --locked ... -- run --provider <p>
--workspace <tmp> --json "$(cat ISSUE.md)"` writing JSON-lines events to `<tmp>/events.jsonl`,
then re-runs the test script and reports PASS/FAIL (dumping the event log to stderr on FAIL).
Cleans up its tempdir via `trap ... EXIT`.

**Makefile (`test` target):** appended a shell guard — `[ -n "$AI_API_KEY" ]` runs
`live_smoke.sh deepseek && live_smoke.sh qwen` after `cargo test --workspace --locked`; otherwise
prints a one-line skip notice. Verified both paths: `make test` with no key runs the full offline
suite (9 tests in `harness-core` + others, all green) and prints the skip line; the guard is a
plain POSIX `[ ]` test so it needs no extra tooling.

**Headless run — already complete, not touched:** `crates/banana-harness/src/main.rs`'s
`headless()` (survey found this before I started) already emits one `serde_json::to_string(&event)`
per line under `--json` and maps `TurnState::{Done,Error,Cancelled}` to exit codes `{0,1,2}`.
Confirmed directly (see below) rather than re-verified through a passing smoke run, since the key
didn't work.

**README (`rust/README.md`, new — the root `README.md` still describes the coexisting TS product
until ticket 15's cutover; this is the Rust-product-specific one):** evaluator quickstart, the
three-layer config, a key-bindings table (checked against `harness-tui/src/ui.rs`'s
`HELP_COMMANDS`/`HELP_KEYS` and `lib.rs`'s `interpret_key` — not guessed), and the guardrails
list. It documents `/undo` and the `permission.mcp`/doom-loop behavior as designed/final-state
(tickets 12 and the merged permission engine), since this ticket merges last in the stated wave-1
order (05, 12, 11, 14) — by the time it lands on `banana/impl` those will be true. If the actual
merge order differs, re-check those two mentions.

**Real-key verification — reported honestly, not fabricated:** the API key supplied for this
session was rejected by **both** providers with HTTP 401:
- Direct `curl` to `https://api.deepseek.com/chat/completions`: `401`, `"Authentication Fails,
  Your api key: ****a3ba is invalid"`.
- Direct `curl` to `https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions`: `401`,
  `"Incorrect API key provided"`.
- Running the actual `banana-harness run --provider deepseek --json` headless binary against the
  fixture reproduces the same thing end to end and confirms our own error handling: one clean
  `{"type":"turn.done","state":"error","error":"provider returned HTTP 401 Unauthorized:
  ... your api key: ****a3ba is invalid ..."}` event, exit code 1, key redacted in the message —
  this matches ticket 01's note that "a bogus key ... returns a clean HTTP 401 turn error"
  exactly, so the plumbing is proven even though the functional fix-the-bug path is not.

This means the acceptance criterion "fixes the fixture's failing test using DeepSeek, and again
using Qwen" is **not verified** — it needs a working key. Everything else (fixture, script,
Makefile wiring, the no-key skip path, headless JSON/exit codes, the README) is done and tested.
`cargo fmt --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` are
clean (no Rust source changed by this ticket, only the fixture/script/README/Makefile).
