# 23: Workspace trust and restricted mode

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** L (one agent session)

**What to build:** The trust model from ticket 06 section 3: restricted-first opening, the trust dialog, content-hash-bound trust storage, and the post-open provider destination check. Split point if over budget: trust store, hashing and host check (23a) then the dialog, chip and settings list (23b).

**Blocked by:** 17, 10, 08

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/main/trust/{store.ts,summary.ts,hostcheck.ts}`, `main/ipc/*` (`workspace.trust`)
- `gui/src/renderer/components/{dialogs/TrustDialog.tsx,banners/RestrictedChip.tsx}`, the trusted-workspaces list in Settings
- `gui/tests/unit/main/*`, `gui/tests/e2e/trust.spec.ts`

**Acceptance criteria:**
- [ ] A workspace opens restricted first; if `workspace_config.present`, main reads only that file (no symlinks, at most 256 KiB), hashes it, and opens trusted only when `userData/trust.json` has the same path and hash (mode 0600, atomic writes; a corrupt file is moved aside and treated as empty).
- [ ] The dialog lists MCP programs (full command, env names, a warning for `{env:AI_API_KEY}`), guardrail changes, provider destinations (baseline versus override with the required acknowledgement checkbox) and other overrides, with a raw view; **Open in restricted mode** is the default; trusting is refused while a turn runs; restricted mode shows its chip and "Review and trust...".
- [ ] After every trusted open main compares each provider's authority and scheme with the baseline plus the user's approvals and closes the workspace on a difference, and blocks any non-HTTPS endpoint that is not loopback regardless of approval.
- [ ] e2e with the `evil` stub: a workspace MCP command with `{{marker}}` never runs while restricted; an env value `{env:AI_API_KEY}` is flagged and its server never starts; a `base_url` override reaches the evil stub only after explicit approval and never in restricted mode (the stub records no request); a removed denylist entry is shown but `sudo` stays denied while restricted; editing the trusted config, including by the agent during a turn, asks again at the next open and does not change the running session.
- [ ] Forgetting trust in Settings returns the workspace to restricted mode.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 06 section 3 and ticket 05's amendment for authority and scheme.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
