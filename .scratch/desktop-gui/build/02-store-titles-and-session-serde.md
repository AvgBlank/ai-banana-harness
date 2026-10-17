# 02: Session serde and one canonical title writer

**Lane / milestone:** R · M1 Core additions   **Size:** S (one agent session)

**What to build:** `harness-store` gets serde on `Session` and a single overwriting title writer, so renaming a session is possible without a second writer. This is the smallest slice of ticket 10 and the base of `rename_session`.

**Blocked by:** 01

**Status:** done 2026-10-17. All five acceptance criteria verified by tests that were run; `make lint` and `make test` passed locally with the build output in a temporary directory outside the repository. Not run: CI on Linux (macOS only here).

**Files and components likely to change:**
- `crates/harness-store/Cargo.toml`, `src/lib.rs` (derive `Serialize` on `Session`; `set_session_title`; delete `set_session_title_if_unset`)
- `crates/harness-store/tests/titles.rs` (new)
- `crates/harness-core/src/harness.rs` (the one caller: the first turn's auto-title, guarded by `title.is_none()` in core)
- `crates/harness-core/tests/sessions.rs`

**Acceptance criteria:**
- [x] `Session` serializes with fields `id, title, workspace, provider, model, created_at, updated_at` (add `Deserialize` only if a consumer needs it).
- [x] `Store::set_session_title(id, title)` overwrites an existing title; `set_session_title_if_unset` is deleted and `git grep set_session_title_if_unset` finds nothing.
- [x] Core still titles a session from its first turn exactly once; a second turn does not change it (seam-1 test).
- [x] No migration is added; `MIGRATIONS` is unchanged.
- [x] A store test shows the overwrite and the JSON shape of a `Session`.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-store --locked
cargo test -p harness-core --test sessions --locked
make lint
make test
```

**Documentation:** None.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.

**Completion record (2026-10-17):**
- Changed: `crates/harness-store/{Cargo.toml,src/lib.rs}` (workspace-pinned `serde`; `Serialize` on `Session`; `set_session_title` replaces `set_session_title_if_unset`, which is deleted), `crates/harness-core/src/harness.rs` (the one caller; the "only if untitled" guard now lives in core, and the session is read once, before titling, instead of after), `Cargo.lock` (harness-store's dependency list: `serde`, and `serde_json` for the shape test; edited by hand and proven consistent by building with `--locked`), `crates/harness-store/tests/titles.rs` (new, 3 tests), `crates/harness-core/tests/sessions.rs` (+1 test).
- Behaviour notes: the writer does not touch `updated_at` (as before; a title is not activity, so a rename does not reorder the session list, and a store test pins it). An unknown session id is a silent no-op, as with the other session setters; ticket 03's `rename_session` is where "session not found" belongs. No `Deserialize` was added: nothing consumes it yet.
- Known ceiling, recorded in the code as a `ponytail:` comment: the check-then-write in `execute` is not atomic, so a rename landing between the check and the write would be overwritten. Turns do not overlap within a session today (the bridge allows one running turn per workspace), so this is theoretical; the upgrade path is one conditional UPDATE.
- Tests run: `cargo test -p harness-store --locked` (4 passed: 3 new, 1 existing), `cargo test -p harness-core --test sessions --locked` (7 passed, including the new one), `make lint` (clean), `make test` (122 passed, 0 failed; live smoke skipped because `AI_API_KEY` was unset on purpose). Mutation check: with the core guard removed, both the new test and the existing "title once" test fail, so they protect the guard; the file was restored byte-for-byte.
- Deviations: none from the ticket.

