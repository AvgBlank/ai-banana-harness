// IPC between the renderer and main: channel names and the strict argument schemas main validates
// before acting. The renderer reaches them only through the frozen `window.banana` object.
import { z } from "zod";
import { type MethodName, methods } from "./protocol";

export const channels = {
  appInfo: "banana:app:info",
  prefsGet: "banana:prefs:get",
  prefsSet: "banana:prefs:set",
  workspacePick: "banana:workspace:pick",
  workspaceOpen: "banana:workspace:open",
  workspaceTrust: "banana:workspace:trust",
  workspaceClose: "banana:workspace:close",
  rpcCall: "banana:rpc:call",
  liveAttach: "banana:live:attach",
  keyStatus: "banana:key:status",
  keySetForSession: "banana:key:set-for-session",
  keyRecheck: "banana:key:recheck",
  externalOpen: "banana:external:open",
  sidecarRestart: "banana:sidecar:restart",
} as const;

/** Main to renderer, one channel each. */
export const pushChannels = {
  sidecarState: "banana:sidecar:state",
  sidecarEvent: "banana:sidecar:event",
  menuCommand: "banana:menu:command",
} as const;

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

export const MAX_INPUT_BYTES = 1024 * 1024;

const path = z.string().min(1).max(4096);

export const ipcArgs = {
  prefsSet: z.object({ theme: z.enum(["system", "dark", "light"]).optional() }),
  workspacePath: z.strictObject({ path }),
  workspaceTrust: z.strictObject({ path, decision: z.enum(["trust", "restricted"]) }),
  rpcCall: z.strictObject({
    method: z.enum(rpcAllowlist),
    params: z.record(z.string(), z.unknown()),
  }),
  liveAttach: z.strictObject({ workspace: path, sessionId: z.string().min(1).max(128) }),
  keyForSession: z.strictObject({ key: z.string().trim().min(1).max(512) }),
  externalOpen: z.strictObject({ url: z.string().min(1).max(2048) }),
};

export type RpcCall = {
  [M in (typeof rpcAllowlist)[number]]: {
    method: M;
    params: z.output<(typeof methods)[M]["params"]>;
  };
}[(typeof rpcAllowlist)[number]];

/** Validates a renderer `rpc.call`: an allowlisted method, params that fit that method, a capped input. */
export function parseRpcCall(raw: unknown): RpcCall {
  const call = ipcArgs.rpcCall.parse(raw);
  const params = methods[call.method].params.parse(call.params);
  if (
    "input" in params &&
    new TextEncoder().encode(String(params.input)).length > MAX_INPUT_BYTES
  ) {
    throw new Error("input is longer than 1 MiB");
  }
  return { method: call.method, params } as RpcCall;
}
