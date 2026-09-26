# Provider selection from AI_API_KEY alone

Type: grilling
Status: resolved
Blocked by: 01, 06

## Question

Given that only `AI_API_KEY` is supplied and it may belong to DeepSeek or Qwen, how does the harness choose provider, endpoint and model without the evaluator editing anything? Options:
- a committed default
- an env override
- an auto-detect probe at startup
- a TUI picker

And how are reasoning-model differences between the two handled behind one provider interface?

Inputs from the API research:
- A DeepSeek probe is free (`/models` returns 401 for other keys).
- Qwen's region can't be inferred from the key.
- DeepSeek has no `seed` and ignores `temperature` while thinking, which conflicts with the brief's reproducibility rule. Decide whether evaluation runs with thinking on, and record it.

Decided by the user (see Confirm the evaluation procedure with the organisers):
- **Provider:** picked in a startup popup, with no auto-detection.
- **Model:** cheap and suitable.
- **Qwen endpoint:** Beijing, `https://dashscope.aliyuncs.com/compatible-mode/v1`.

Proposed models: `deepseek-flash` and `qwen3.8-flash`.

Still open:
- **Thinking and reproducibility:** thinking on or off during evaluation.
- **Text-only rule:** `deepseek-flash` is listed as vision-capable, while `deepseek-v4-pro` is text-only. Does the brief's "text-only language models" rule force v4-pro?
- **Remembering the choice:** should the popup's choice be remembered between runs?
- **Region fallback:** a Singapore host for a Qwen key that turns out to be international.

## Answer

The user decided this on 2026-09-27:

- **Choosing the provider:** a popup at startup lets you pick DeepSeek or Qwen. There's no auto-detection, and `AI_API_KEY` is sent to whichever you pick.
- **DeepSeek:** `deepseek-flash` at `https://api.deepseek.com`.
- **Qwen:** `qwen3.8-flash` at `https://dashscope.aliyuncs.com/compatible-mode/v1` (Beijing).
- **Thinking is off for both** (DeepSeek `thinking` disabled, Qwen `enable_thinking: false`). That simplifies the client:
  - no `reasoning_content` to echo back in history
  - `temperature` works again
  - there are no `tool_choice` restrictions

  Reproducibility: commit `temperature` (plus `seed` on Qwen) in config. DeepSeek has no seed, so document that it can't be fully deterministic.
- **Not handled:** Coding Plan (`sk-sp-`) keys and a Singapore fallback. The user expects a working key.
