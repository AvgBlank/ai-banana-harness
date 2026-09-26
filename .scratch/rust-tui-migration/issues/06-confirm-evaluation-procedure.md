# Confirm the evaluation procedure with the organisers

Type: task
Status: resolved
Blocked by:

## Question

HITL, for the user: ask the hackathon organisers, then record their answers here.

1. How exactly is the evaluation issue supplied to the running TUI: typed or pasted by a human, a URL, a file?
2. Is the target repository already checked out, and where? Will `WORKSPACE` be honoured?
3. Which OS and architecture will the evaluator use? Is a Rust toolchain present? Is network access available during `make setup`?
4. Which provider and model will `AI_API_KEY` belong to: DeepSeek or Qwen, which model id, and which region endpoint?
5. Are there time or token limits per evaluation run?
6. (Added after the API research.) If it's Qwen: which region is the key for (Singapore `dashscope-intl` or Beijing `dashscope`, or a workspace host)? And is it a Coding Plan `sk-sp-` key? Those keys forbid non-interactive use.

## Answer

The user relayed the organisers' answers on 2026-09-27:

1. **How the issue arrives:** a human types or pastes it into the TUI.
2. **Workspace:** `WORKSPACE` will be honoured.
3. **Evaluator's machine:**
   - OS: macOS or Linux.
   - Rust: may or may not be installed.
   - Internet: available. `make setup` checks for Rust and installs it with rustup if it's missing.
4. **Provider:** chosen by the user in a **startup popup** (Qwen or DeepSeek). Use whichever model is suitable and cheap.
5. **Limits:** no per-run time or token limits.
6. **Qwen region:** probably **Beijing** (`dashscope.aliyuncs.com`). The Coding Plan (`sk-sp-`) question wasn't answered.
