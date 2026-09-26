# 14: Evaluation readiness: live smoke test and README

**What to build:**

**Fixture:** a small repository with one failing test and an issue text describing the bug.

**Live smoke script:** runs a headless run (JSON lines) against the fixture with the chosen provider and asserts the test passes afterwards. `make test` runs it only when `AI_API_KEY` is set.

**Headless run:** JSON-lines output and exit codes (0 done, 1 error, 2 cancelled or limit reached) are finalised.

**README:** the evaluator steps (export key, `make setup`, `make run`, `WORKSPACE`), the config layers, the key bindings and the guardrails.

**Blocked by:** 04

**Status:** ready-for-agent

- [ ] With a real key, `make test` fixes the fixture's failing test using DeepSeek, and again using Qwen.
- [ ] Without a key, `make test` skips the live smoke and passes.
- [ ] Headless JSON output is one valid JSON event per line, and the exit codes match the outcome.
- [ ] The README contains no credentials and matches actual behaviour.
