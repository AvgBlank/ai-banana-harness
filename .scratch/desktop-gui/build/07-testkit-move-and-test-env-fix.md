# 07: harness-testkit: move the fake provider; NO_PROXY in make test

**Lane / milestone:** K · M2 Test foundation   **Size:** M (one agent session)

**What to build:** Create `crates/harness-testkit` as the single owner of the fake provider (ticket 05), move the existing fake and its fixtures into it without changing behaviour, keep it out of the shipped build with `default-members`, and add the verified `NO_PROXY` expression to `make test`.

**Blocked by:** 01

**Status:** done

**Files and components likely to change:**
- `crates/harness-testkit/` (library: `FakeProvider`, `sse_text`, `sse_tool_calls`, `fixture()`, `TEST_API_KEY`, the 10 `.sse` files); depends on no product crate; I/O errors in the server loop are handled without panics
- `crates/harness-core/tests/common/mod.rs` (keeps `TestHarness`, uses the testkit), `crates/harness-core/tests/fixtures/` (moved)
- `Cargo.toml` (`harness-testkit` in `[workspace.dependencies]`; `default-members` listing the product crates), `Cargo.lock`
- `Makefile` (`TEST_ENV` expression on the `cargo test` line)

**Acceptance criteria:**
- [x] Every existing core test passes unchanged apart from imports (baseline: `plain_answer` 4 of 4).
- [x] `HTTP_PROXY=http://127.0.0.1:9 make test` passes the offline suite, and with `NO_PROXY=corp.example.com` set the user's entry is preserved (effective `NO_PROXY=corp.example.com,127.0.0.1,localhost`).
- [x] `make setup` does not compile `harness-testkit`; `cargo tree -p banana-harness --edges normal` does not list it; it appears only under `[dev-dependencies]`.
- [x] `git grep -n FakeProvider` finds the definition only in `crates/harness-testkit`.
- [x] A server loop client disconnect does not panic.

**Verification** (commands to run; none has been run by the planning):

```
HTTP_PROXY=http://127.0.0.1:9 http_proxy=http://127.0.0.1:9 make test
cargo tree -p banana-harness --edges normal | grep -c testkit   # 0
cargo build --release --locked -v 2>&1 | grep -c harness-testkit   # 0
make lint
make test
```

**Documentation:** Applies from the drafts: `AGENTS.md` crate-boundaries testkit sentence, the "no test hooks" bullet, the fake-provider one-owner example and the seam-1 wording; the `CONTRIBUTING.md` layout line for `harness-testkit`.

**Notes:** The Makefile fix is independent of the GUI and protects the evaluation path.

**Completion record:**
- `crates/harness-testkit` (library; depends on `serde_json` and `tokio` only) owns `FakeProvider`, `Request`, `sse_text`, `sse_tool_calls`, `TEST_API_KEY`, `fixture()`, the `git` and `commit_all` helpers and the 10 `.sse` files (`crates/harness-testkit/fixtures/`). The server loop returns `io::Result` and the connection task's result is ignored, so a client hang-up is not a panic; an `InFlight` guard keeps `max_in_flight` correct on that path. `TestHarness` and `collect` stay in core's `tests/common/mod.rs`. The 15 test files that import the helpers now import them from the testkit (no re-exports).
- `Cargo.toml`: `default-members` lists the seven product crates; `harness-testkit` is in `[workspace.dependencies]` and is a `dev-dependency` of `harness-core` only. `Cargo.lock` was edited by hand and builds with `--locked`.
- `Makefile`: `TEST_ENV` on the `cargo test` line only (not the live smoke).
- Docs applied: `AGENTS.md` (testkit boundary sentence, no-test-hooks bullet, seam-1 wording, one-owner example) and `CONTRIBUTING.md` (layout line, test description). Ticket 08 completes the testkit description.
- Verified: `make lint` and `make test` pass (170 tests, 169 before plus the new disconnect test; `AI_API_KEY` unset, so the live smoke was skipped); `make test` passes with six proxy variables pointing at `127.0.0.1:9` and `NO_PROXY=corp.example.com`; negative controls fail (`cargo test` without the fix: failures; `make test TEST_ENV=`: 5 failures); effective `NO_PROXY` through the real Makefile is `127.0.0.1,localhost` when unset and `corp.example.com,127.0.0.1,localhost` when set; `make test` passes on a PATH with no `node`; `cargo tree -p banana-harness --edges normal | grep -c testkit` is 0; `workspace_default_members` has 7 entries without the testkit; debug and release `cargo build -v` mention the testkit 0 times. Mutation: an `unwrap()` on the SSE write fails the disconnect test; restored byte for byte.
- Not run: Linux CI, the live smoke, `make setup` itself (its build command was run as `cargo build --release --locked -v`).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
