// The strict argument schemas main validates before acting on a renderer call. The renderer
// reaches them only through the frozen `window.banana` object (channel names: ./channels).
import { z } from "zod";
import { type MethodName, methods } from "./protocol";

/** A failure that crosses the process boundary as `{ code, message }`, never as a stack trace. */
export class IpcError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "IpcError";
  }
}

/** What an invoke handler returns: the value, or the error the renderer is allowed to see. */
export type IpcResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: { code: string; message: string } };

/** The only methods the renderer may call. Opening and closing a workspace belong to the trust flow. */
export const rpcAllowlist = [
  "session.list",
  "session.create",
  "session.resume",
  "session.rename",
  "session.switch_provider",
  "session.undo",
  "provider.list",
  "turn.start",
  "turn.cancel",
  "action.reply",
  "changes.workspace",
  "changes.turn",
  "workspace.list",
] as const satisfies readonly MethodName[];

type Allowed = (typeof rpcAllowlist)[number];

export const MAX_INPUT_BYTES = 1024 * 1024;
/** Every string param except a turn's input and a typed answer. */
export const MAX_STRING_LENGTH = 4096;
const MAX_ANSWER_BYTES = 64 * 1024;

const path = z.string().min(1).max(MAX_STRING_LENGTH);

export const ipcArgs = {
  /** Unknown keys are dropped, not refused: a newer renderer may send more than main knows. */
  prefsPatch: z.object({
    theme: z.enum(["system", "dark", "light"]).optional(),
    sidebarOpen: z.boolean().optional(),
    changesOpen: z.boolean().optional(),
  }),
  keyForSession: z.strictObject({ key: z.string().trim().min(1).max(512) }),
  workspacePath: z.strictObject({ path }),
  rpcCall: z.strictObject({ method: z.string(), params: z.record(z.string(), z.unknown()) }),
  externalOpen: z.strictObject({ url: z.string().min(1).max(MAX_STRING_LENGTH) }),
};

export type RpcCall = {
  [M in Allowed]: { method: M; params: z.output<(typeof methods)[M]["params"]> };
}[Allowed];

const bytes = (text: string) => new TextEncoder().encode(text).length;

function isAllowed(method: string): method is Allowed {
  return (rpcAllowlist as readonly string[]).includes(method);
}

/**
 * Validates a renderer `rpc.call`: an allowlisted method (`method_not_allowed` otherwise), params
 * that fit that method with no extra keys, and bounded strings.
 */
export function parseRpcCall(raw: unknown): RpcCall {
  const call = ipcArgs.rpcCall.safeParse(raw);
  if (!call.success) throw new IpcError("invalid_arguments", "the call has the wrong shape");
  const { method } = call.data;
  if (!isAllowed(method)) throw new IpcError("method_not_allowed", `${method} cannot be called`);
  const strict = z.strictObject(methods[method].params.shape);
  const parsed = strict.safeParse(call.data.params);
  if (!parsed.success) throw new IpcError("invalid_arguments", `invalid params for ${method}`);
  const params = parsed.data as Record<string, unknown>;
  for (const [key, value] of Object.entries(params)) {
    if (typeof value === "string") {
      const limit = key === "input" ? MAX_INPUT_BYTES : MAX_STRING_LENGTH;
      if (bytes(value) > limit) throw new IpcError("invalid_arguments", `${key} is too long`);
    } else if (key === "answer" && typeof value === "object" && value && "text" in value) {
      if (bytes(String(value.text)) > MAX_ANSWER_BYTES) {
        throw new IpcError("invalid_arguments", "the answer is too long");
      }
    }
  }
  return { method, params } as RpcCall;
}
