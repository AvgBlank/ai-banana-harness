# Commands can run in a container, one per command

Accepted. This adds a way to run the agent's `bash` commands that ADR 0001's local-first design did not have; it does not bring back the hosted product's sandbox provider.

## Decision

- **A command runner seam.** `bash` runs through a `CommandRunner` in `harness-tools`. The host runner is the default and is what ran commands before. The **container runner** runs each command in a container of its own through the `docker` command-line client: no Docker library, no socket handed to anything.
- **Only commands are isolated.** The harness, the file tools (`read_file`, `write_file`, `edit`, `glob`, `grep`) and the workspace's guardrails, approvals, trust and restricted mode stay as they are, on the host. A blocked command never reaches Docker.
- **One container per command**, made with `docker create`, run with `docker start -a`, read with `docker inspect` and removed with `docker rm -f`. Killing the `docker` client does not stop what runs in the container (measured), so a timeout or a cancel runs `docker kill` on the container itself.
- **The image is chosen explicitly and must already be on the machine.** Banana Harness never pulls an image: a missing one fails the command with the `docker pull` to run. Nothing falls back to the host when Docker or the image is missing.
- **Hardening by default:** no network (an explicit opt-in turns it on), a read-only root filesystem with a 256 MiB `/tmp`, all capabilities dropped, `no-new-privileges`, 2 GiB of memory without swap, 256 processes, the host's uid and gid, and an environment of `HOME=/tmp` only, so the API key is never in a container. The workspace is the one bind mount, at its own path.
- **Cleanup is guaranteed three ways:** the container is removed on every path; every container carries a label naming the process that made it; opening a harness with the container runner removes the labelled containers of owners that no longer run (a sidecar killed with SIGKILL cannot clean up after itself).
- **Cancellation reaches a running command** on both runners (a turn cancel used to wait for the command).
- **Selection is not user-visible yet.** A harness is opened with a container configuration by the program that embeds it; no config key, Settings control or protocol field exists, so a workspace config cannot choose or loosen the runner. When a key is added it is read from the embedded defaults and the user-level config only.

## Consequences

- A command that needs the network, a tool the image lacks, or a cache under `$HOME` (lost after each command) fails or is slow until the image, the network setting or the workspace is adjusted. Docker carries a command's two output streams separately, so their order is not kept as a terminal would.
- It is not a sandbox for the agent. The agent still edits the workspace, and a mounted workspace is writable from the container.
- A SIGKILLed sidecar leaves its running commands (host: a process group; container: a container) until the next sweep or a manual `docker rm -f $(docker ps -aq --filter label=banana-harness.command)`.
- Docker Desktop on macOS maps file ownership itself; a Linux daemon applies the uid literally. The uid and ownership tests run against Docker on Linux CI before that is claimed.
