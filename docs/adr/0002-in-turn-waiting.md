# Paused turns wait in place

In TrueForge, a turn that needs a tool approval or a user's answer ends as `done` with `required_actions`, and the next turn rebuilds every thread from a stored snapshot. Banana Harness instead keeps the turn alive: the waiting thread blocks on a channel until the TUI replies.

TrueForge's design exists for a stateless, multi-replica HTTP server. Running in-process makes it unnecessary, and dropping it removes the snapshot-rebuild, completion-replay and resume-validation paths, roughly a third of the loop.

The cost: if the process dies while a turn is waiting, that turn can't be resumed at that exact point. The session continues from its last persisted context instead.
