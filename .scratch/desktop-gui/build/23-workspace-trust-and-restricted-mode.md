# 23: Workspace trust and restricted mode

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** L (one agent session)

**What to build:** The trust model from ticket 06 section 3: restricted-first opening, the trust dialog, content-hash-bound trust storage, and the post-open provider destination check. Split point if over budget: trust store, hashing and host check (23a) then the dialog, chip and settings list (23b).

**Blocked by:** 17, 10, 08

**Status:** done

**Files and components likely to change:**
- `gui/src/main/trust/{store.ts,summary.ts,hostcheck.ts}`, `main/ipc/*` (`workspace.trust`)
- `gui/src/renderer/components/{dialogs/TrustDialog.tsx,banners/RestrictedChip.tsx}`, the trusted-workspaces list in Settings
- `gui/tests/unit/main/*`, `gui/tests/e2e/trust.spec.ts`

**Acceptance criteria:**
- [x] A workspace opens restricted first; if `workspace_config.present`, main reads only that file (no symlinks, at most 256 KiB), hashes it, and opens trusted only when `userData/trust.json` has the same path and hash (mode 0600, atomic writes; a corrupt file is moved aside and treated as empty).
- [x] The dialog lists MCP programs (full command, env names, a warning for `{env:AI_API_KEY}`), guardrail changes, provider destinations (baseline versus override with the required acknowledgement checkbox) and other overrides, with a raw view; **Open in restricted mode** is the default; trusting is refused while a turn runs; restricted mode shows its chip and "Review and trust...".
- [x] After every trusted open main compares each provider's authority and scheme with the baseline plus the user's approvals and closes the workspace on a difference, and blocks any non-HTTPS endpoint that is not loopback regardless of approval.
- [x] e2e with the `evil` stub: a workspace MCP command with `{{marker}}` never runs while restricted; an env value `{env:AI_API_KEY}` is flagged and its server never starts; a `base_url` override reaches the evil stub only after explicit approval and never in restricted mode (the stub records no request); a removed denylist entry is shown but `sudo` stays denied while restricted; editing the trusted config, including by the agent during a turn, asks again at the next open and does not change the running session.
- [x] Forgetting trust in Settings returns the workspace to restricted mode.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 06 section 3 and ticket 05's amendment for authority and scheme.

**Completion record:**
- Main: `gui/src/main/trust/` holds `store.ts` (`trust.json` in `userData`: version 1, path to config hash, approved destinations and time; mode 0600, written to a temp file and renamed, writes queued; an unreadable or wrong-shape file is renamed to `trust.json.corrupt-<time>` and nothing is trusted), `summary.ts` (`readConfig`: a regular file of at most 256 KiB whose folder and file are not symlinks, hashed with SHA-256; `summarize`: the advisory reading with `smol-toml`) and `hostcheck.ts` (provider destinations against the baseline and approvals; a non-HTTPS endpoint that is not loopback is refused whatever was approved). `flow.ts` is the one place that decides trust: open restricted first, upgrade only when the stored path and hash match, check destinations after the trusted open and close the workspace on a difference (`blocked_config`), fall back to restricted when the config fails to load. Review, trust, list and forget are new IPC channels; the renderer sends a path, a reviewed hash and an acknowledgement, never content or a trust flag, and `trust_workspace_config` still never crosses the boundary. `OpenedWorkspace.restricted` became `trust` (`none`, `trusted`, `needs_review`, `unreadable`). `smol-toml` 1.9.1 was added as an exact devDependency.
- Renderer: `TrustDialog` (MCP programs with full command, env names and a warning for `{env:AI_API_KEY}` or any env expansion; guardrail rules with `allow` on `bash`/`external_directory` in red; destinations against the baseline with the "My API key will be sent to ..." checkbox; other overrides; raw view; **Open in restricted mode** focused), `RestrictedChip` with "Review and trust..." in the title bar while a config is untrusted, a dismissible notice when main blocks a workspace, and a "Trusted workspaces" list with Forget in Settings. The dialog opens when a folder is picked or chosen; a restored or session-opened workspace shows the chip only. After the sidecar restarts the renderer opens its known workspaces again, so trust is judged again against the file as it is.
- Checked: `make gui-lint`; `make gui-test`; `make gui-build`; `make gui-e2e` (34 passed, 7 new in `trust.spec.ts` on the real sidecar with the `evil` stub: a `{marker}` MCP command and an `{env:AI_API_KEY}` server never run while restricted and are flagged; a `base_url` override reaches the evil stub only after the acknowledgement and never when restricted; a stored entry that approved nothing is blocked after the open with no request sent and the workspace closed; `sudo` stays denied while restricted although the config allows it; the agent editing the trusted config leaves the running session alone and the next launch asks again, while the same bytes open trusted; Forget returns to restricted; trusting is refused by main while a gated turn runs and the workspace stays restricted); `make lint`; `make test`. Mutations caught: the hash comparison, the destination check and the acknowledgement requirement.
- Deviations: (1) the chip appears only when a config exists that is not trusted, not for every restricted workspace. (2) An already open trusted workspace is not re-hashed when opened again in the same run; the next real open is. (3) Forgetting is refused while a turn runs, like trusting. (4) Forgetting while a turn runs is covered by unit tests only. (5) A symlink and an oversized config are covered by unit tests only. (6) The `clipOutput` speed-up (ticket 20 code) was committed on its own as `perf(gui): optimize output clipping for large text`.
- Reviewed risks, not changed: a turn could start between the running-turn check and the close that follows it (the close cancels it); if forgetting cannot close the workspace, the entry is already gone while the loaded config stays until the next open; the file is hashed just before the sidecar reads it (the existing ponytail). An open workspace keeps the config it was opened with, which is what was trusted; re-picking it does not re-hash it.
- Not covered: Linux; the trust of sessions on screen elsewhere.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
