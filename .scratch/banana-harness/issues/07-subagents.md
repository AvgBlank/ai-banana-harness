# 07: Dynamic subagents

**What to build:** the root-only create-subagent tool (name plus task), so depth is 1.

**What a child gets:**
- the subagent identity prompt
- its task as the only message
- the parent's tools
- no question tool and no subagent tool

**Scheduling:**
- Only leaf threads run: a parent with live children waits.
- At most five run at once.
- Events from concurrent threads fan into one stream with thread ids.
- A child's final text becomes the parent's tool result.
- Thread-created and thread-done events are emitted.

**In the TUI:** subagent cards nested in the transcript, and a subagent tree (a simple list is fine until 13's sidebar).

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] Seam-1 test: the root spawns two children that run concurrently. Each child's answer arrives as the parent's tool result, and then the parent answers.
- [ ] Seam-1 test: when seven children are requested, no more than five run at the same time.
- [ ] Seam-1 test: a child can't call the subagent or question tools.
- [ ] Seam-2 test: a subagent card renders under its parent.
