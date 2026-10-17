// The sidecar protocol v1 (crates/harness-bridge): every frame, method and event as zod schemas.
// Unknown fields are ignored (the protocol adds fields without a version bump); the golden frames
// under crates/harness-bridge/tests/fixtures are checked against these schemas by the contract test.
import { z } from "zod";

export const PROTOCOL_VERSION = 1;

const id = z.string();
const provider = z.enum(["deepseek", "qwen"]);

export const errorCodes = [
  "invalid_request",
  "unknown_method",
  "invalid_params",
  "workspace_not_found",
  "workspace_not_open",
  "workspace_mismatch",
  "session_not_found",
  "turn_in_progress",
  "no_active_turn",
  "unknown_action",
  "api_key_missing",
  "internal",
] as const;

// ---- entities (core and store types, serialized as they are) ----

export const session = z.object({
  id,
  title: z.string().nullable(),
  workspace: z.string(),
  provider: z.string(),
  model: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
});

const usage = z.object({
  prompt_tokens: z.number(),
  completion_tokens: z.number(),
  total_tokens: z.number(),
});

const turnState = z.enum(["done", "error", "cancelled"]);

const modelOutput = z.object({
  content: z.string(),
  finish_reason: z.string(),
  usage: usage.nullable(),
});

const threadId = z.string();

const eventSchemas = {
  "turn.created": z.object({ session_id: id, turn_id: id }),
  "turn.done": z.object({ state: turnState, error: z.string().optional() }),
  "turn.undone": z.object({ turn_id: id }),
  "model.message": z.object({
    thread_id: threadId,
    message_id: id,
    message: modelOutput.optional(),
  }),
  "model.message.delta": z.object({ thread_id: threadId, message_id: id, content: z.string() }),
  "tool.call": z.object({
    thread_id: threadId,
    tool_call_id: id,
    name: z.string(),
    arguments: z.string(),
  }),
  "tool.response": z.object({
    thread_id: threadId,
    tool_call_id: id,
    content: z.string(),
    blocked: z.boolean(),
    diff: z.string().optional(),
  }),
  "context.compacted": z.object({ thread_id: threadId, usage }),
  "mcp.auth_required": z.object({ thread_id: threadId, server: z.string(), url: z.string() }),
  "thread.created": z.object({ thread_id: threadId, parent: z.string(), title: z.string() }),
  "thread.done": z.object({ thread_id: threadId, error: z.string().optional() }),
  "tool.approval_required": z.object({
    action_id: id,
    thread_id: threadId,
    tool_call_id: id,
    name: z.string(),
    arguments: z.string(),
    rule: z.string(),
  }),
  "tool.response_required": z.object({
    action_id: id,
    thread_id: threadId,
    question: z.string(),
    options: z.array(z.string()),
  }),
} as const;

export type EventKind = keyof typeof eventSchemas;
export const eventKinds = Object.keys(eventSchemas) as EventKind[];

/** A core `Event`: `{id, type, ...fields}`. A `type` that is not listed fails validation. */
export const event = z.union(
  eventKinds.map((kind) =>
    z.object({ id, type: z.literal(kind) }).extend(eventSchemas[kind].shape),
  ) as unknown as [z.ZodType, z.ZodType, ...z.ZodType[]],
) as z.ZodType<{ id: string; type: EventKind } & Record<string, unknown>>;

const turnRecord = z.object({
  turn_id: id,
  seq: z.number(),
  input: z.string(),
  state: turnState.nullable(),
  error: z.string().nullable(),
  usage: usage.nullable(),
  events: z.array(event),
});

export const providerOption = z.object({
  provider,
  model: z.string(),
  host: z.string(),
  scheme: z.string(),
});

const fileChange = z.object({
  path: z.string(),
  status: z.enum(["added", "modified", "deleted", "renamed"]),
  added: z.number(),
  removed: z.number(),
  patch: z.string().nullable(),
  truncated: z.boolean(),
});

const answer = z.union([
  z.enum(["allow", "allow_for_session", "deny"]),
  z.object({ text: z.string() }),
]);

// ---- methods ----

const workspaceParams = z.object({ workspace: z.string() });
const sessionParams = workspaceParams.extend({ session_id: id });
const empty = z.object({});

/** Every method with its params and result. */
export const methods = {
  "session.list": { params: empty, result: z.object({ sessions: z.array(session) }) },
  "workspace.open": {
    params: z.object({
      path: z.string(),
      trust_workspace_config: z.boolean().optional(),
      provider: provider.optional(),
    }),
    result: z.object({
      workspace: z.string(),
      already_open: z.boolean(),
      provider,
      workspace_config: z.object({
        present: z.boolean(),
        loaded: z.boolean(),
        path: z.string(),
      }),
    }),
  },
  "workspace.close": { params: workspaceParams, result: empty },
  "workspace.list": {
    params: empty,
    result: z.object({
      workspaces: z.array(
        z.object({
          workspace: z.string(),
          provider,
          active_turn: z
            .object({
              session_id: id,
              turn_id: id.nullable(),
              pending_action_ids: z.array(id),
            })
            .nullable(),
        }),
      ),
    }),
  },
  "provider.list": {
    params: workspaceParams,
    result: z.object({ providers: z.array(providerOption), default: provider }),
  },
  "session.create": {
    params: workspaceParams.extend({ provider: provider.optional() }),
    result: z.object({ session }),
  },
  "session.resume": {
    params: sessionParams,
    result: z.object({ session, turns: z.array(turnRecord) }),
  },
  "session.rename": {
    params: sessionParams.extend({ title: z.string() }),
    result: z.object({ session }),
  },
  "session.switch_provider": {
    params: sessionParams.extend({ provider }),
    result: z.object({ session }),
  },
  "session.undo": {
    params: sessionParams,
    result: z.object({ outcome: z.enum(["restored", "nothing_to_undo"]), event: event.optional() }),
  },
  "turn.start": { params: sessionParams.extend({ input: z.string() }), result: empty },
  "turn.cancel": { params: sessionParams, result: z.object({ cancelled: z.boolean() }) },
  "action.reply": {
    params: sessionParams.extend({ action_id: id, answer }),
    result: empty,
  },
  "changes.workspace": {
    params: workspaceParams,
    result: z.object({ git: z.boolean(), files: z.array(fileChange) }),
  },
  "changes.turn": {
    params: sessionParams,
    result: z.object({ checkpoint: z.boolean(), files: z.array(fileChange) }),
  },
  shutdown: { params: empty, result: empty },
} as const;

export type MethodName = keyof typeof methods;
export const methodNames = Object.keys(methods) as MethodName[];
export type Params<M extends MethodName> = z.input<(typeof methods)[M]["params"]>;
export type Result<M extends MethodName> = z.output<(typeof methods)[M]["result"]>;

// ---- frames ----

export const readyFrame = z.object({
  kind: z.literal("ready"),
  protocol: z.number(),
  app: z.string(),
  version: z.string(),
  api_key_present: z.boolean(),
});

export const fatalFrame = z.object({
  kind: z.literal("fatal"),
  error: z.object({ code: z.string(), message: z.string() }),
});

export const requestFrame = z.object({
  kind: z.literal("request"),
  id,
  method: z.string(),
  params: z.record(z.string(), z.unknown()),
});

const errorBody = z.object({
  code: z.string(),
  message: z.string(),
  data: z.record(z.string(), z.unknown()).optional(),
});

export const responseFrame = z.union([
  z.object({ kind: z.literal("response"), id, ok: z.literal(true), result: z.unknown() }),
  z.object({
    kind: z.literal("response"),
    id: id.nullable(),
    ok: z.literal(false),
    error: errorBody,
  }),
]);

export const eventFrame = z.object({
  kind: z.literal("event"),
  workspace: z.string(),
  session_id: id,
  turn_id: id.nullable(),
  event,
});

/** What the sidecar sends. */
export const sidecarFrame = z.union([readyFrame, fatalFrame, responseFrame, eventFrame]);

export type Session = z.infer<typeof session>;
export type ProviderOption = z.infer<typeof providerOption>;
export type ReadyFrame = z.infer<typeof readyFrame>;
export type FatalFrame = z.infer<typeof fatalFrame>;
export type ResponseFrame = z.infer<typeof responseFrame>;
export type EventFrame = z.infer<typeof eventFrame>;
export type SidecarFrame = z.infer<typeof sidecarFrame>;
