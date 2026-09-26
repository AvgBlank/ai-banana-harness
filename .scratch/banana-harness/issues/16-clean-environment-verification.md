# 16: Clean-environment verification

**What to build:** follow the brief's evaluator workflow exactly, on a fresh Linux container and a fresh macOS user account:

1. Clone.
2. Export the key.
3. `make setup`.
4. `make run`.
5. Choose a provider and a workspace, then paste a real GitHub issue from a small public repository.
6. Confirm the harness resolves it.
7. `make test`.

Do this with both providers. Record anything that required a manual step, and fix it or document it.

**Blocked by:** 15

**Status:** ready-for-human

- [ ] Every step succeeds on Linux without manual intervention.
- [ ] Every step succeeds on macOS without manual intervention.
- [ ] One real issue is resolved with DeepSeek, and one with Qwen.
- [ ] Findings are written up in the ticket, with any fixes merged.
