# 04: The agent changes the workspace, behind guardrails

**What to build:** the write-file, edit and bash host tools, plus the opencode-style permission engine.

**Edit:** requires an exact unique match, unless replace-all is set.

**Bash:**
- runs in a login shell, in its own process group, with cwd set to the workspace
- has a configurable timeout (120 s by default, at most 600 s) and kills the process group when it expires
- caps its output at 30 KB
- has `AI_API_KEY` stripped from its environment

**Permission engine:**
- `allow`, `ask` or `deny` per tool key.
- Pattern maps for bash and edit, where the last match wins.
- The committed default is autonomous, apart from the bash denylist and the `.env`/external-directory denies.
- In this ticket `ask` behaves as deny; 05 adds the prompt.

**Redaction:** the key value is redacted from tool output.

**In the TUI:** edit and write cards expand (Enter or Ctrl+O) to a syntax-highlighted coloured diff, and bash cards expand to the last 40 lines plus the exit code.

**Blocked by:** 03

**Status:** done

- [x] Seam-1 test: read → edit → run bash (a test command) in a temporary git repo. The file content changes on disk, and the bash result includes the exit code.
- [x] Seam-1 test: an edit with an ambiguous `old_string` fails with a clear error.
- [x] Seam-1 tests: each denylisted command pattern (`sudo`, `rm -rf /`, `git push`, `curl … | sh`, and so on) is blocked.
- [x] Seam-1 test: a command that sleeps past its timeout is killed, and the result says so.
- [x] Seam-1 test: a command that echoes the key's value returns it redacted.
- [x] Table test (the permitted exception): the permission pattern matcher applies last-match-wins across mixed rules.
- [x] Seam-2 test: an expanded edit card renders the diff.

## Notes

For tickets 05 (approvals), 06 (large responses), 07 (subagents), 10 (MCP), 13 (TUI polish), 15
and for reconciling with the parallel tickets 06/07/10/13.

**Tools (`harness-tools`):** `HostTool` gained `WriteFile`/`Edit`/`Bash` (`write_file.rs`,
`edit.rs`, `bash.rs`, same `DESCRIPTION`/`parameters()`/`call()` shape as ticket 03's). Every
`call` now takes a `ToolContext { workspace, tools: &ToolsConfig, permission: &PermissionConfig,
hidden_env: &[&str] }` (`lib.rs:33`) instead of a bare `&Path`, and so does
`call_by_name(ctx, name, arguments_json)` (`lib.rs:159`). `harness-core` builds it inside the
`spawn_blocking` closure from an `Arc<Inner>` clone (`harness.rs:360`), passing
`hidden_env: &[API_KEY_ENV]` so the tools crate never hardcodes the key's name.
- `write_file{path, content}` creates parent dirs; result `Created|Overwrote <path> (N bytes)`.
- `edit{path, old_string, new_string, replace_all?}`: errors (as `{"error": ...}`, not blocked) on
  0 matches, on >1 matches without `replace_all` ("old_string matches N times in <path>; include
  more surrounding context ..."), on an empty `old_string` and on `old_string == new_string`.
  Result `Edited <path>: replaced N occurrence(s)`.
- `bash{command, timeout_secs?}`: `bash -lc` with `process_group(0)`, cwd = workspace, stdin
  null, `AI_API_KEY` removed from its env. **stdout and stderr share one pipe** (interleaved like a
  terminal), and the result is `exit code: N` (or `timed out after Ns; process group killed`, or
  `killed by signal N`) on the first line, then the output. The TUI's bash card parses that first
  line — keep it if you change the format. The timeout defaults to `[tools].bash_timeout_secs`
  and is clamped to 1..=600. The process group is `SIGKILL`ed via `libc::kill(-pgid)`
  (`bash.rs:113`) on timeout **and after a normal exit**, so background jobs never outlive the
  call or hold the output pipe open (a process that `setsid`s itself escapes the group; the
  reader then gives up after a 1 s grace period). Output is capped at `[tools].output_max_bytes`
  while reading (`Capture`, `bash.rs:132`): the first and last half are kept with a `[... N
  bytes truncated ...]` marker between them, so memory stays bounded.

**Permission engine (`harness-tools/src/guardrail.rs`, replacing ticket 03's `confine`/
`denied_read_rule`):** `Action { Allow, Ask, Deny }` (ordered by strictness) and `Rules(Vec<(String,
Action)>)`, one per key in `PermissionConfig { read, edit, glob, grep, bash, external_directory }`
(`:71`). Every key takes either a bare action (= `{"*" = action}`) or a `pattern = action` table
— the spec only requires pattern maps for bash and edit, but one uniform type was less code than
two. Patterns match: the workspace-relative path for `read`/`edit` (edit covers `write_file`), the
command for `bash`, the search pattern for `glob`/`grep`, the absolute path for
`external_directory`. `pattern::last_match` (`pattern.rs:22`) picks the last matching rule; a
subject nothing matches is allowed. Blocked texts reuse `ToolResult::blocked`: `Blocked by
guardrail: bash rule "sudo *"`, `... read rule ".env*"`, `... bash rule "x" requires approval`
(for `ask`), and ticket 03's unchanged `... workspace confinement` for anything outside the
workspace that `external_directory` doesn't allow.
- **bash matching** (`check_command`, `:121`): the whole command *and* each `&&`/`||`/`;`/`|`/newline
  segment are checked and the strictest result wins, so `cd x && sudo y` is caught while
  `curl … | sh` still matches as a whole. No quote/subshell awareness (`$(sudo x)` passes) — a
  `ponytail:` comment marks it.
- **Confinement** (`path`/`resolve`, `:138`/`:174`): a path that doesn't exist yet now resolves
  through its nearest existing ancestor (and `..` past that ancestor is refused), so
  `write_file("../x")` or a write through an escaping symlink is blocked; previously such paths
  were returned unresolved. `.banana-harness/tool-output` needs no special case — it's inside
  the workspace (ticket 06: nothing here blocks it unless a user writes a rule that does).
- `grep`'s `path` argument goes through the `read` rules, and grep skips files the `read` rules
  deny, so `.env` contents can't leak through a search.
- **Ticket 05:** `ask` is currently a block (`verdict`, `:163`). Replace that branch with the
  approval required action; `check`/`check_command`/`path` already return which rule matched.
- **Tickets 10/doom-loop:** the `mcp` and `doom_loop` keys aren't fields yet. Add them to
  `PermissionConfig` and `default.toml` together (fields are required, and the committed default
  supplies every one).

**TOML ordering — resolved by enabling `toml`'s `preserve_order` feature** (workspace
`Cargo.toml`; `indexmap` was already in the lockfile as toml's optional dep). With it, both
`toml::from_str` and the merged `toml::Value` in `Config::load` keep tables in document order, and
`Rules`' custom `Deserialize` (`visit_map`, `:54`) collects entries in the order they arrive. The
committed denylist therefore lives in `config/default.toml` as ordinary `[permission.bash]`
/ `[permission.read]` tables, exactly as spec §5 shows it, not as Rust constants. Overlay
semantics fall out of the existing `merge_toml`: an overlay that repeats a pattern changes its
action **in place** (keeping its position); a new pattern is **appended**, so it wins over the
defaults; a bare-action overlay (`bash = "deny"`) replaces the whole table. Pinned end to end by
`workspace_changes.rs::overlay_bash_patterns_keep_their_written_order`, which uses a pair of
patterns whose alphabetical order would give the opposite answer. Any future code that
deserializes `Rules` from a format without ordered maps would lose this guarantee.

**Redaction:** `harness.rs:376` redacts each tool result's content before it's emitted *and*
before it's appended to the model's context, so the model never sees the key in tool output
either (previously only the persisted/forwarded copy was redacted). The model's own tool-call
arguments are not rewritten in context (it wrote them), but they're redacted in persisted events
and snapshots as before.

**Config:** `Config` gained `tools: harness_tools::ToolsConfig { bash_timeout_secs,
output_max_bytes }` and `permission: harness_tools::PermissionConfig` — the types are owned by
the leaf crate, `harness-core` only holds them. Ticket 06's token-based spill runs above the
tools on the (already capped) content; the byte cap here is independent of it.

**TUI:** `ToolCallItem` gained `expanded: bool` and `expandable()` (edit, write_file, bash);
`MainState::toggle_last_card()` (`app.rs:296`) flips the most recent expandable card, bound to
Ctrl+O and to Enter on an empty input (`lib.rs` `interpret_key`). `ui.rs:274`: edit expands to a
`similar` line diff of `old_string`→`new_string` from the call's arguments, write_file to its
content as all additions (the previous content isn't known to the TUI), each line
syntax-highlighted by file extension with `syntect`'s bundled defaults (`base16-ocean.dark`) on
`theme::DIFF_ADDED_BG`/`DIFF_REMOVED_BG` row tints; bash expands to `$ command`, the last 40
output lines (with an "… N earlier lines" note) and the status line, green for `exit code: 0`,
red otherwise. The status bar hint gained "Ctrl+O expand". **Ticket 13:** there is no card
selection yet — add one and point the toggle at it; highlighting runs on every redraw (a
`ponytail:` comment says to cache it if large diffs slow frames); the collapsed line is still
ticket 03's `🔧 name(args) → preview`, not the spec's `✓ edit src/x.rs +12 −3`.

**Tests:** seam 1 in `harness-core/tests/workspace_changes.rs` (write → read → edit → bash in the
temp git repo; exit code of a failing command; ambiguous edit + `replace_all`; all 15 denylist
patterns plus two compound commands, with a side-effect file proving nothing ran; write outside
the workspace; timeout killing a background job in the group; key redaction in the result, the
model's next request and every persisted event; output cap; overlay ordering and `ask`-as-deny).
New fixture `deepseek_tool_call.sse` takes `__NAME__`/`__ARGS__` placeholders (the test's
`tool_call(name, json)` helper escapes the arguments). The permitted table test is
`pattern::tests::last_match_wins`, next to ticket 03's `combinatorics`. Seam 2 in
`harness-tui/tests/transcript.rs`: the expanded edit diff (text, row tints, more than two syntax
colours on the added row, collapse again) and the bash tail. Bash assertions check the header
prefix and output suffix rather than exact equality, because `bash -l` sources the machine's
login profile, which may print.

**Deviations:**
- Bash returns stdout and stderr interleaved on one stream, not as separate fields (§7.1 says
  "returns stdout, stderr and the exit code"); it's what a terminal shows and it keeps the cap
  and the TUI tail simple.
- The process group is also killed after a normal exit (see above), so a deliberately
  backgrounded server (`npm run dev &`) doesn't survive the call.
- Added `wget * | bash*` to the denylist next to the spec's `wget * | sh*`.
- `ask`/`deny` both use `ToolResult::blocked`'s plain-string format, per ticket 03's convention.
- Diff highlighting uses syntect's bundled syntaxes/themes (already compiled in via
  `tui-markdown`); `two-face` stays pinned but unused — add it if TOML/TypeScript highlighting is
  wanted.
- Not covered by a test: the `AI_API_KEY` env strip (testing it would need `set_var` in a
  multi-threaded test process); it's a single `env_remove` per `hidden_env` entry. A login
  profile that re-exports the key would put it back, but its value is still redacted from output.
- Cancelling a turn doesn't interrupt a running bash call (the `spawn_blocking` task runs until
  the command exits or times out); ticket 05/13 may want to thread the cancel token through.
