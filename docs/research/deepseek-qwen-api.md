# DeepSeek and Qwen (Model Studio) chat APIs: behaviour for a hand-written Rust client

Researched 2026-09-27 against live first-party docs; every claim links its source. "Observed" means I sent an unauthenticated or bogus-key `curl` probe on 2026-09-27; no valid key was used, so nothing that needs a real key has been tested. Context7's indexes for both providers were out of date (still describing DeepSeek V3.2), so the live docs are the authority here.

## TL;DR

- Both providers speak OpenAI Chat Completions over `Authorization: Bearer <key>`, SSE `data:` lines, and end the stream with `data: [DONE]`. One client can serve both, with a per-provider "dialect" layer.
- **Reasoning must go back in history** on both, which differs from OpenAI: DeepSeek returns 400 if `reasoning_content` is missing on any request that has `tools`. Qwen 3.8 has `preserve_thinking` on by default and says to send it back in `reasoning_content`, not inside `content`.
- Thinking is **on by default** on the current flagship models from both. When thinking is on, both reject `tool_choice: "required"` and `tool_choice` naming a specific function.
- Reproducibility: Qwen supports `seed`. DeepSeek has **no `seed`**, and `temperature` does nothing in thinking mode.
- Detecting the provider: `GET https://api.deepseek.com/models` is free and returns 401 for a non-DeepSeek key. A `sk-sp-` prefix means a Qwen **Coding Plan** key, which uses a different base URL. Otherwise both providers issue `sk-...` keys, so the prefix doesn't tell them apart.

## 1. Base URLs, regions, auth

| | DeepSeek | Qwen / Model Studio (DashScope) |
|---|---|---|
| OpenAI-format base | `https://api.deepseek.com` (single global host) [ds-first] | Per region, see below. Path suffix `/compatible-mode/v1` [qw-ref] |
| Chat endpoint | `POST /chat/completions` [ds-chat] | `POST {base}/chat/completions` [qw-ref] |
| Other formats | Anthropic at `https://api.deepseek.com/anthropic`, plus the Responses API [ds-pricing] | Anthropic at `{host}/apps/anthropic`, plus Responses [qw-regions] |
| Beta features | `https://api.deepseek.com/beta` (strict tools, prefix completion) [ds-tools] | n/a |
| Auth | `Authorization: Bearer $KEY` [ds-first] | `Authorization: Bearer $KEY` [qw-ref] |
| `/v1` prefix | `https://api.deepseek.com/v1/models` also routes (observed 401, not 404). The docs only show the bare host | Required: base ends `/compatible-mode/v1` [qw-compat] |

**Qwen regions** [qw-regions]. Keys are **region-bound**: using a key against another region's host returns `401 invalid_api_key` "Incorrect API key provided" [qw-compat].

| Region | Workspace host (recommended) | Legacy DashScope host |
|---|---|---|
| Singapore (intl) | `{WorkspaceId}.ap-southeast-1.maas.aliyuncs.com` | `dashscope-intl.aliyuncs.com` |
| China (Beijing) | `{WorkspaceId}.cn-beijing.maas.aliyuncs.com` | `dashscope.aliyuncs.com` |
| China (Hong Kong) | `{WorkspaceId}.cn-hongkong.maas.aliyuncs.com` | `cn-hongkong.dashscope.aliyuncs.com` |
| US (Virginia) / Frankfurt / Tokyo | `{WorkspaceId}.{us-east-1,eu-central-1,ap-northeast-1}.maas.aliyuncs.com` | none |
| Coding Plan (`sk-sp-` keys) | `https://coding-intl.dashscope.aliyuncs.com/v1` (OpenAI), `/apps/anthropic` [qw-cp] | n/a |

- Workspace hosts need a `WorkspaceId`, which the key alone doesn't give you. Legacy DashScope hosts accept keys for "all workspaces". They "remain fully functional", but they "will no longer support new features after September 30, 2026". Timeouts are 600 s on the legacy hosts versus 3600 s on workspace hosts [qw-regions].
- Doc inconsistencies (flagged):
  - The compat page lists Beijing as `…ap-southeast-1…` (a typo) and Virginia as `dashscope-us.aliyuncs.com` [qw-compat].
  - The regions page says Virginia has no DashScope host [qw-regions], yet `dashscope-us.aliyuncs.com` still answers with an OpenAI-shaped 401 (observed).
- Coding Plan terms forbid "automated scripts, application backends, or other non-interactive scenarios". Its supported models are qwen3.7-plus, qwen3.6-plus, qwen3.5-plus, qwen3-coder-plus/next, qwen3-max-2026-01-23, kimi-k2.5, glm-5, glm-4.7 and MiniMax-M2.5 [qw-cp].

## 2. Current models, context, max output

| Model id | Context | Max output | Thinking | Source |
|---|---|---|---|---|
| `deepseek-flash` (DeepSeek-V4.1-Flash, vision) | 1M (1,048,576) | 384K (393,216) | hybrid, default on, effort `high` | [ds-pricing], [ds-models] |
| `deepseek-v4-pro` (V4-Pro-0813, text-only) | 1M | 384K | hybrid, default on | [ds-pricing], [ds-models] |
| `qwen3.8-max` (+ `qwen3.8-max-0902`) | 1,000,000 (input ≤ 991,808; ≤ 983,616 when thinking) | 131,072; CoT ≤ 262,144 | hybrid, default on | [qw-38max] |
| `qwen3.8-flash` | same as above | 131,072 | hybrid, default on | [qw-38flash] |
| `qwen3.7-plus` (Alibaba's pick for coding tools) | 1,000,000 | 131,072 | hybrid, default on | [qw-37plus], [qw-textgen] |

- DeepSeek defaults for `max_tokens` are 8K when not thinking, 64K when thinking, and 128K at `reasoning_effort: max` [ds-chat].
- Qwen defaults `max_tokens` to the model's maximum output. For thinking models, `max_completion_tokens` covers CoT plus answer, while `max_tokens` covers the answer only (except the DeepSeek and GLM models Qwen hosts) [qw-ref].
- **Legacy DeepSeek names:** `deepseek-chat` and `deepseek-reasoner` were due to be discontinued on 2026-07-24 [ds-changelog]. `deepseek-v4-flash` is temporarily routed to V4.1-Flash [ds-pricing]. I did not verify whether the old names still resolve.
- Model Studio also hosts DeepSeek, Kimi, GLM and MiniMax models [qw-textgen], so a model id doesn't identify the provider.

## 3. Streaming (SSE)

Both send `data: {chat.completion.chunk}` lines and finish with `data: [DONE]` [ds-chat] [qw-ref]. Differences:

| | DeepSeek | Qwen |
|---|---|---|
| Keep-alive | `: keep-alive` SSE comments while queued. Non-streaming responses get blank lines. The server closes the connection if inference hasn't started within 10 min [ds-rate] | not documented |
| Usage in stream | Usage rides on the **last content chunk** (the one with `finish_reason`); there is no separate usage chunk. With `include_usage: true`, every chunk carries `usage: null` except the last. Sending `stream_options` without `stream: true` returns 400 [ds-chat] | Needs `stream_options.include_usage: true`. Usage then arrives in an **extra final chunk with `choices: []`** [qw-ref] |
| Delta fields | `content`, `reasoning_content`, `role`, `tool_calls` [ds-chat] | `content`, `reasoning_content`, `role` (first chunk only), `tool_calls`, plus `function_call`/`refusal`/`audio`, which are null [qw-ref] |
| `finish_reason` | `stop`, `length`, `content_filter`, `tool_calls`, **`insufficient_system_resource`**, **`aborted`** [ds-chat] | `stop`, `length`, `tool_calls` [qw-ref] |
| Non-stream timeout | n/a | At least 300 s; on timeout it **returns the partial content instead of an error** [qw-ref] |

Parsing rules:
- Ignore `:` comment lines and blank lines.
- Accept `null` or absent fields interchangeably.
- Tolerate `choices: []`.
- Make usage optional on any chunk.

## 4. Tool calling

| | DeepSeek | Qwen |
|---|---|---|
| Tool schema | OpenAI `{"type":"function","function":{name,description,parameters,strict?}}`. Name matches `[A-Za-z0-9_-]`, max 128 [ds-chat] | Same shape. Name max 64; the name `search` is rejected [qw-ref] [qw-err] |
| Multiple calls per turn | Supported: `tool_calls` is an array with `index`, and `auto` lets the model call "one or more tools". There is no `parallel_tool_calls` parameter [ds-chat] | **`parallel_tool_calls` defaults to `false`**. Set it to `true` to get multiple calls [qw-ref] [qw-tools] |
| Streamed arguments | The first chunk for each call carries `id`, `type` and `function.name`. Later chunks carry only `function.arguments` fragments, keyed by `index` [ds-chat] | Name arrives in the first chunk. Later chunks carry `id: ''` (empty string, not absent) and `type: 'function'`. The name and arguments may be in separate chunks. Concatenate by `index` [qw-tools]. With `tool_stream: false` (the default), array/object parameters are emitted **in one piece** [qw-ref] |
| `tool_choice` | `none`/`auto`/`required`/`{function}`. Defaults to `auto` when tools are present. **`required` or a named function in thinking mode returns 400** [ds-chat] | `auto` (default), `none`, `{function}`. Qwen models **don't support `required`**. In thinking mode a named function fails too [qw-ref] [qw-tools] |
| Strict JSON schema | `strict: true` on the `/beta` base URL, with a restricted schema subset [ds-tools] | not investigated |
| History rules | Tool calls can't be injected mid-conversation in Chat Completions (use the Anthropic or Responses API for that). System messages can go mid-conversation [ds-tools] | Every `tool_call_id` must be answered by exactly one `tool` message before the next user message. Tool `content` must be a string [qw-err] [qw-ref]. The compat page says "only `messages[0]` supports the system role" [qw-compat] |
| Argument validity | "Does not always generate valid JSON". Validate before use [ds-chat] | Same warning [qw-ref] |

Doc conflict: the older Qwen compat page says `tools` can't be combined with `stream=True` [qw-compat]. The current function-calling guide documents streamed tool calls [qw-tools], so treat the compat-page line as stale. This is unverified without a key.

## 5. Reasoning / thinking

| | DeepSeek | Qwen |
|---|---|---|
| Toggle | `"thinking": {"type": "enabled"\|"disabled"}` (default enabled), or `reasoning_effort: "none"` [ds-think] [ds-chat] | `"enable_thinking": true\|false` at the top level of the JSON body. Qwen 3.8 also accepts `reasoning_effort: "none"`, which maps to off [qw-ref] |
| Effort | `reasoning_effort`: `low`/`high`/`max`. Aliases: `minimal`→low; `medium`/`xhigh`→high; `ultra`→max [ds-think] | Qwen 3.8: `low`/`medium`/`xhigh` (default xhigh). Aliases: `high`/`max`→xhigh. Alternatively `thinking_budget` in tokens (default 131,072). **Setting both is an error** [qw-ref] |
| Output field | `reasoning_content`, alongside `content`, in both `message` and `delta` [ds-think] | `reasoning_content` [qw-think] |
| Send back in history? | **With `tools`: all prior `reasoning_content` must be sent back, even on turns with no tool call, or you get 400.** Without `tools` it is ignored [ds-think] | `preserve_thinking` defaults to **true on qwen3.8-max/flash** and false elsewhere. Send `reasoning_content` on assistant messages and **never merge it into `content`**. It is billed as input. Omitting it doesn't error [qw-ref] [qw-think] |
| Restrictions | Thinking ignores `temperature`, `presence_penalty` and `frequency_penalty` (silently, no error) [ds-think] | JSON mode plus thinking returns 400. Some open-source models support thinking only when streaming [qw-err] [qw-think] |

## 6. Sampling and reproducibility

| | DeepSeek | Qwen |
|---|---|---|
| `temperature` | 0–2, default 1. **No effect when thinking** [ds-chat] | [0, 2) [qw-ref] |
| `top_p` | Only used when thinking, and clamped to 0.95–1.0. Fixed at 1.0 when not thinking [ds-chat] | (0, 1] [qw-ref] |
| `seed` | **Not in the request schema** [ds-chat]. I didn't test whether it's ignored or rejected | `seed`, 0 to 2³¹−1, "same result as much as possible" [qw-ref] |
| Other | `frequency_penalty`/`presence_penalty` are deprecated no-ops [ds-chat] | `top_k`, `repetition_penalty` (non-OpenAI, top-level). `presence_penalty` defaults to 1.5 for qwen3.8 when not thinking [qw-ref] |

Takeaway: fully deterministic output isn't achievable on DeepSeek. For the closest you can get, disable thinking and set `temperature: 0`. Qwen offers a best-effort `seed`.

## 7. Rate limits and errors

- **DeepSeek** limits concurrency, not RPM: 2,500 concurrent requests per account on `deepseek-flash` and 500 on `deepseek-v4-pro`. Going over returns HTTP 429 [ds-rate].
  - Status codes: 400 bad format, 401 bad key, 402 insufficient balance, 422 bad params, 429, 500, 503 overloaded [ds-err].
  - Observed body: `{"error":{"message":"Authentication Fails, …","type":"authentication_error","param":null,"code":"invalid_request_error"}}`. Responses carry an `x-ds-trace-id` header. No rate-limit headers were observed on the 401.
- **Qwen**: qwen3.8-max uses dynamic TPM tiers (by monthly spend) in Beijing and Singapore; elsewhere it gets 30,000 RPM and 5M TPM [qw-38max].
  - 429 variants: `Throttling.RateQuota` (RPM), `Throttling.BurstRate` (sudden spikes, back off smoothly), `Throttling.AllocationQuota`/`insufficient_quota` (TPM), plus billing states such as `PrepaidBillOverdue` and `BudgetLimitExceeded` [qw-err].
  - OpenAI-compat error body: `{"error":{"message","type":"invalid_request_error","param","code":"invalid_api_key"},"request_id":…}` [qw-compat] (confirmed by observation).
  - Observed inconsistency: `cn-hongkong.dashscope.aliyuncs.com` returned the **DashScope-native** shape `{"request_id","code":"InvalidApiKey","message"}` even on the compat path.
- **Unverified for both:** how an error that occurs *mid-stream* is framed, and whether `Retry-After` or rate-limit headers are sent.

## 8. Known incompatibilities with the OpenAI spec (summary)

1. Both send `reasoning_content` in responses and expect it back on assistant messages. It isn't an OpenAI field.
2. Parameters outside the OpenAI spec:
   - DeepSeek: `thinking`, `user_id`, `prefix`.
   - Qwen: `enable_thinking`, `thinking_budget`, `preserve_thinking`, `top_k`, `repetition_penalty`, `tool_stream`, `enable_search`.
   - All of these go at the top level of the JSON body (the SDKs' `extra_body`) [ds-think] [qw-ref].
3. DeepSeek puts usage on the final content chunk. Qwen and OpenAI use a separate `choices: []` chunk.
4. DeepSeek adds finish reasons `insufficient_system_resource` and `aborted`.
5. Qwen sends `id: ""` on continuation tool-call deltas. Qwen defaults `parallel_tool_calls` to false and doesn't support `tool_choice: "required"`.
6. In thinking mode, neither provider accepts `tool_choice: "required"` or a named function.
7. DeepSeek has no `seed`, and its thinking mode silently ignores sampling parameters.
8. DeepSeek usage adds `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens` [ds-chat]. Qwen usage adds `prompt_tokens_details.cached_tokens` and `completion_tokens_details.reasoning_tokens`/`text_tokens` [qw-ref].
9. Qwen `/models` on the Coding Plan host returns camelCase `ownedBy`, and returned 200 **even with a bogus key** (observed).

## 9. Detecting a key's provider cheaply

- **Key format:**
  - Qwen general keys "start with `sk-`". Coding Plan and Token Plan keys start with `sk-sp-` [qw-err] [qw-cp].
  - DeepSeek's key format isn't documented (**unverified**; it's commonly `sk-` + 32 hex). The prefix therefore only reliably identifies `sk-sp-`.
- **DeepSeek probe:** `GET https://api.deepseek.com/models` is documented, costs no tokens, and returns `context_window`, `max_output_tokens` and supported effort levels per model [ds-models]. A foreign key gets 401 (observed).
  - `GET /user/balance` also exists [ds-first].
- **Qwen probe:** `GET {legacy-host}/compatible-mode/v1/models` is a real route. Observed: 401 with a bad key, 404 for an unknown path. It isn't documented, and its 200 body with a valid key is **unverified**. A region mismatch returns `401 invalid_api_key` [qw-compat], so try Singapore (`dashscope-intl`) first, then Beijing.
  - For `sk-sp-` keys, `/v1/models` is unauthenticated, so it can't validate the key. Validate with a 1-token chat call to a plan model instead.

## Implications for the Rust client

1. **One OpenAI-shaped core plus a `Dialect` enum** (`DeepSeek`, `QwenIntl`, `QwenCn`, `QwenCodingPlan`). The dialect decides the base URL, the thinking toggle field (`thinking` vs `enable_thinking`), the effort vocabulary, `parallel_tool_calls: true` (Qwen), whether to send `seed` (Qwen only), and where usage is read from.
2. **Startup detection:**
   - `sk-sp-` → Coding Plan.
   - Otherwise try `GET api.deepseek.com/models`, then `GET dashscope-intl…/compatible-mode/v1/models`, then `dashscope.aliyuncs.com`. Pick the first that returns 200.
   - Cache the result, and allow an optional base-URL override for workspace hosts and the Virginia/Frankfurt/Tokyo regions. All endpoints return an OpenAI-style 401 on mismatch.
3. **Serde model:**
   - Use `Option` for every delta field.
   - Deserialize `tool_calls[].id` as `Option<String>` and treat `""` as `None`. Never overwrite an accumulated id or name with an empty one.
   - Accumulate calls in a `BTreeMap<index, PartialCall>`.
   - Accept unknown `finish_reason` strings as `Other(String)`.
   - Keep `#[serde(flatten)] extra` for vendor usage fields.
4. **History:** store `reasoning_content` on every assistant turn and always send it back as `reasoning_content`, never inside `content`. DeepSeek requires this whenever tools are sent (otherwise 400), and qwen3.8 expects it by default. For long sessions this costs input tokens (billed on Qwen). A later compaction step may drop reasoning from old turns, but only for DeepSeek requests that carry no tools. Needs testing.
5. **Never send `tool_choice: "required"` or a named function** while thinking is on. Default to `auto`.
6. **SSE reader:** skip `:` comment lines and blank lines, stop at `[DONE]`, and allow `choices: []`. Take usage from whichever chunk has a non-null `usage`. Always stream: Qwen's non-streaming timeout silently truncates output. Allow at least 10 min of idle time because of DeepSeek's queueing.
7. **Errors:**
   - Parse `{"error":{message,type,code}}`, with fallbacks to the top-level `{code,message,request_id}` shape and to raw text.
   - 429 and 503 (plus Qwen `Throttling.*` and DeepSeek `insufficient_system_resource`) are retryable with jittered backoff. 401, 402 and 400 are fatal and should be shown to the user.
   - Log `x-ds-trace-id` / `request_id`.
8. **Reproducibility / evals:** this can't be guaranteed on DeepSeek. Record the model id, the `system_fingerprint` (DeepSeek only; Qwen returns null) and the parameters, rather than promising determinism.
9. **Defaults:**
   - DeepSeek: `deepseek-flash`, thinking on, `reasoning_effort: high`, `max_tokens` left unset or ≤ 393,216.
   - Qwen: `qwen3.7-plus` or `qwen3.8-flash`, thinking on, `max_completion_tokens` ≤ 131,072.
   - Read context and output limits from DeepSeek `/models` at runtime. For Qwen, use a static table.

## Open questions / not verified

- Anything that needs a real key: the Qwen `/models` 200 body, whether `deepseek-chat` still resolves, DeepSeek's behaviour with `seed`, and whether Qwen's `tools` + `stream` works on every current model.
- How errors that occur mid-stream are framed on both providers, and whether rate-limit or `Retry-After` headers exist.
- DeepSeek's key format.
- Whether a hackathon Qwen key will be a Coding Plan key (whose terms forbid non-interactive use) or a pay-as-you-go key.

## Sources

- **ds-first**: <https://api-docs.deepseek.com/> ("Your First API Call")
- **ds-pricing**: <https://api-docs.deepseek.com/quick_start/pricing>
- **ds-chat**: <https://api-docs.deepseek.com/api/create-chat-completion>
- **ds-models**: <https://api-docs.deepseek.com/api/list-models>
- **ds-think**: <https://api-docs.deepseek.com/guides/thinking_mode>
- **ds-tools**: <https://api-docs.deepseek.com/guides/tool_calls>
- **ds-rate**: <https://api-docs.deepseek.com/quick_start/rate_limit>
- **ds-err**: <https://api-docs.deepseek.com/quick_start/error_codes>
- **ds-changelog**: <https://api-docs.deepseek.com/updates>
- **qw-compat**: <https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope>
- **qw-ref**: <https://www.alibabacloud.com/help/en/model-studio/qwen-api-via-openai-chat-completions>
- **qw-regions**: <https://www.alibabacloud.com/help/en/model-studio/regions>
- **qw-think**: <https://www.alibabacloud.com/help/en/model-studio/deep-thinking>
- **qw-tools**: <https://www.alibabacloud.com/help/en/model-studio/qwen-function-calling>
- **qw-err**: <https://www.alibabacloud.com/help/en/model-studio/error-code>
- **qw-textgen**: <https://www.alibabacloud.com/help/en/model-studio/text-generation-model>
- **qw-38max**: <https://www.alibabacloud.com/help/en/model-studio/qwen3-8-max>
- **qw-38flash**: <https://www.alibabacloud.com/help/en/model-studio/qwen3-8-flash>
- **qw-37plus**: <https://www.alibabacloud.com/help/en/model-studio/qwen3-7-plus>
- **qw-cp**: <https://www.alibabacloud.com/help/en/model-studio/coding-plan>

[ds-first]: https://api-docs.deepseek.com/
[ds-pricing]: https://api-docs.deepseek.com/quick_start/pricing
[ds-chat]: https://api-docs.deepseek.com/api/create-chat-completion
[ds-models]: https://api-docs.deepseek.com/api/list-models
[ds-think]: https://api-docs.deepseek.com/guides/thinking_mode
[ds-tools]: https://api-docs.deepseek.com/guides/tool_calls
[ds-rate]: https://api-docs.deepseek.com/quick_start/rate_limit
[ds-err]: https://api-docs.deepseek.com/quick_start/error_codes
[ds-changelog]: https://api-docs.deepseek.com/updates
[qw-compat]: https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope
[qw-ref]: https://www.alibabacloud.com/help/en/model-studio/qwen-api-via-openai-chat-completions
[qw-regions]: https://www.alibabacloud.com/help/en/model-studio/regions
[qw-think]: https://www.alibabacloud.com/help/en/model-studio/deep-thinking
[qw-tools]: https://www.alibabacloud.com/help/en/model-studio/qwen-function-calling
[qw-err]: https://www.alibabacloud.com/help/en/model-studio/error-code
[qw-textgen]: https://www.alibabacloud.com/help/en/model-studio/text-generation-model
[qw-38max]: https://www.alibabacloud.com/help/en/model-studio/qwen3-8-max
[qw-38flash]: https://www.alibabacloud.com/help/en/model-studio/qwen3-8-flash
[qw-37plus]: https://www.alibabacloud.com/help/en/model-studio/qwen3-7-plus
[qw-cp]: https://www.alibabacloud.com/help/en/model-studio/coding-plan
