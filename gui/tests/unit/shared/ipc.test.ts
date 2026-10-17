import { expect, test } from "vitest";
import { channels, pushChannels } from "../../../src/shared/channels";
import {
  IpcError,
  ipcArgs,
  MAX_INPUT_BYTES,
  MAX_STRING_LENGTH,
  parseRpcCall,
  rpcAllowlist,
} from "../../../src/shared/ipc";

function codeOf(run: () => unknown): string | undefined {
  try {
    run();
  } catch (error) {
    return error instanceof IpcError ? error.code : "other";
  }
  return undefined;
}

const base = { workspace: "/w", session_id: "s" };

test("channel names are unique and namespaced", () => {
  const all = [...Object.values(channels), ...Object.values(pushChannels)];
  expect(new Set(all).size).toBe(all.length);
  for (const name of all) expect(name).toMatch(/^banana:[a-z]+:[a-z-]+$/);
});

test("the renderer cannot reach the trust flow, shutdown or unknown methods", () => {
  for (const method of ["workspace.open", "workspace.close", "shutdown", "files.read", ""]) {
    expect(
      codeOf(() => parseRpcCall({ method, params: {} })),
      method,
    ).toBe("method_not_allowed");
  }
  expect(rpcAllowlist).not.toContain("workspace.open");
  expect(rpcAllowlist).not.toContain("workspace.close");
  expect(rpcAllowlist).not.toContain("shutdown");
  expect(rpcAllowlist).toHaveLength(13);
});

test("every allowlisted method is accepted with fitting params", () => {
  const params: Record<string, Record<string, unknown>> = {
    "session.list": {},
    "workspace.list": {},
    "session.create": { workspace: "/w" },
    "provider.list": { workspace: "/w" },
    "changes.workspace": { workspace: "/w" },
    "session.resume": base,
    "session.undo": base,
    "turn.cancel": base,
    "changes.turn": base,
    "session.rename": { ...base, title: "t" },
    "session.switch_provider": { ...base, provider: "qwen" },
    "turn.start": { ...base, input: "hi" },
    "action.reply": { ...base, action_id: "a", answer: "allow" },
  };
  expect(Object.keys(params).sort()).toEqual([...rpcAllowlist].sort());
  for (const [method, p] of Object.entries(params)) {
    expect(parseRpcCall({ method, params: p }).method, method).toBe(method);
  }
});

test("params are strict: extra keys and a smuggled trust flag are rejected", () => {
  const turn = { ...base, input: "hi" };
  expect(codeOf(() => parseRpcCall({ method: "turn.start", params: { ...turn, x: 1 } }))).toBe(
    "invalid_arguments",
  );
  expect(
    codeOf(() =>
      parseRpcCall({
        method: "session.create",
        params: { workspace: "/w", trust_workspace_config: true },
      }),
    ),
  ).toBe("invalid_arguments");
  expect(
    codeOf(() =>
      parseRpcCall({ method: "session.list", params: { trust_workspace_config: true } }),
    ),
  ).toBe("invalid_arguments");
  expect(codeOf(() => parseRpcCall({ method: "turn.start", params: { workspace: "/w" } }))).toBe(
    "invalid_arguments",
  );
  expect(codeOf(() => parseRpcCall({ method: "turn.start", params: turn, extra: 1 }))).toBe(
    "invalid_arguments",
  );
  expect(codeOf(() => parseRpcCall("nope"))).toBe("invalid_arguments");
});

test("strings are bounded: 4096 characters in general, 1 MiB for a turn's input", () => {
  const long = "a".repeat(MAX_STRING_LENGTH + 1);
  expect(
    codeOf(() => parseRpcCall({ method: "session.create", params: { workspace: long } })),
  ).toBe("invalid_arguments");
  expect(
    codeOf(() => parseRpcCall({ method: "session.rename", params: { ...base, title: long } })),
  ).toBe("invalid_arguments");
  const ok = { ...base, input: "a".repeat(MAX_INPUT_BYTES) };
  expect(codeOf(() => parseRpcCall({ method: "turn.start", params: ok }))).toBeUndefined();
  const big = { ...base, input: "a".repeat(MAX_INPUT_BYTES + 1) };
  expect(codeOf(() => parseRpcCall({ method: "turn.start", params: big }))).toBe(
    "invalid_arguments",
  );
  const answer = { ...base, action_id: "a", answer: { text: "a".repeat(70_000) } };
  expect(codeOf(() => parseRpcCall({ method: "action.reply", params: answer }))).toBe(
    "invalid_arguments",
  );
});

test("argument schemas reject unknown keys, empty and oversized values", () => {
  expect(
    ipcArgs.workspacePath.safeParse({ path: "/w", trust_workspace_config: true }).success,
  ).toBe(false);
  expect(ipcArgs.workspacePath.safeParse({ path: "" }).success).toBe(false);
  expect(ipcArgs.externalOpen.safeParse({ url: `https://a/${"x".repeat(5000)}` }).success).toBe(
    false,
  );
  expect(ipcArgs.externalOpen.safeParse({ url: "https://a", extra: 1 }).success).toBe(false);
});
