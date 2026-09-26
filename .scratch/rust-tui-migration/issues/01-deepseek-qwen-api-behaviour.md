# DeepSeek and Qwen API behaviour

Type: research
Status: resolved
Blocked by:

## Question

How do the DeepSeek and Qwen (Alibaba DashScope / Model Studio) chat APIs behave, in the detail a hand-written Rust client needs? Cover:

- base URLs, including international and China regions
- OpenAI-compatibility level and auth header
- streaming SSE delta format
- tool/function calling: parallel calls, streamed argument chunks, `tool_choice`
- reasoning/thinking output fields, e.g. DeepSeek `reasoning_content` and Qwen thinking mode, and whether reasoning must be sent back in history
- context window and max output per current model
- `temperature` / `seed` support for reproducibility
- rate-limit and error shapes
- known incompatibilities with the OpenAI spec
- whether a key's provider can be detected cheaply, e.g. `GET /models`

## Research

Findings: branch `research/deepseek-qwen-api`, file `docs/research/deepseek-qwen-api.md` (commit 926fae37).

## Answer

Both providers speak OpenAI Chat Completions with `Bearer` auth and SSE streams that end in `[DONE]`, so one hand-written client can serve both, with a small set of per-provider quirks. The details are in `docs/research/deepseek-qwen-api.md` on the `research/deepseek-qwen-api` branch.

**DeepSeek:**
- A single host, `api.deepseek.com`.
- Models are `deepseek-flash` (V4.1) and `deepseek-v4-pro`, both with 1M context and 384K max output.
- Thinking is on by default. While thinking, `temperature` has no effect, and there is no `seed`.
- Usage arrives on the last content chunk.

**Qwen:**
- Keys are tied to a region: `dashscope-intl` (Singapore) or `dashscope` (Beijing). The recommended per-workspace hosts need an ID the key doesn't reveal.
- `sk-sp-` Coding Plan keys use a separate base URL, and their terms forbid non-interactive use.
- qwen3.8 and qwen3.7-plus have 1M context and 131K max output.
- `seed` is supported.
- `parallel_tool_calls` defaults to false.
- Follow-up tool-call chunks carry `id: ""`, so they have to be merged by index.

**Both providers:**
- Each expects `reasoning_content` echoed back on earlier assistant turns. DeepSeek returns a 400 if it's missing when `tools` are present. Qwen 3.8 has `preserve_thinking` on by default.
- With thinking on, `tool_choice` can't be `required` or name a specific function.

**Detection:** `GET api.deepseek.com/models` costs nothing and returns 401 for a non-DeepSeek key. The `sk-sp-` prefix is the only key format that reveals the provider.

**Unverified, since there was no real key:**
- what Qwen's `/models` returns with a valid key
- whether `deepseek-chat` still exists
- how DeepSeek treats `seed`
- how errors are framed mid-stream
- whether rate-limit headers are sent
- DeepSeek's key format

The Qwen docs contradict themselves on some region URLs and on whether `tools` works with streaming.
