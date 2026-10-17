import { expect, test } from "vitest";
import {
  channels,
  ipcArgs,
  MAX_INPUT_BYTES,
  parseRpcCall,
  pushChannels,
  rpcAllowlist,
} from "../../../src/shared/ipc";

test("channel names are unique and namespaced", () => {
  const all = [...Object.values(channels), ...Object.values(pushChannels)];
  expect(new Set(all).size).toBe(all.length);
  for (const name of all) expect(name).toMatch(/^banana:[a-z]+:[a-z-]+$/);
});

test("the renderer cannot reach the trust flow, shutdown or unknown methods", () => {
  for (const method of ["workspace.open", "workspace.close", "shutdown", "files.read"]) {
    expect(() => parseRpcCall({ method, params: {} }), method).toThrow();
  }
  expect(rpcAllowlist).not.toContain("workspace.open");
  expect(rpcAllowlist).not.toContain("shutdown");
});

test("rpc params must fit the method and no extra keys are accepted", () => {
  const params = { workspace: "/w", session_id: "s", input: "hi" };
  expect(parseRpcCall({ method: "turn.start", params }).method).toBe("turn.start");
  expect(() => parseRpcCall({ method: "turn.start", params: { workspace: "/w" } })).toThrow();
  expect(() =>
    parseRpcCall({ method: "turn.start", params, trust_workspace_config: true }),
  ).toThrow();
});

test("a turn input over 1 MiB is refused", () => {
  const base = { workspace: "/w", session_id: "s" };
  const ok = { ...base, input: "a".repeat(MAX_INPUT_BYTES) };
  expect(() => parseRpcCall({ method: "turn.start", params: ok })).not.toThrow();
  const big = { ...base, input: "a".repeat(MAX_INPUT_BYTES + 1) };
  expect(() => parseRpcCall({ method: "turn.start", params: big })).toThrow();
});

test("argument schemas reject unknown keys, empty and oversized values", () => {
  expect(
    ipcArgs.workspacePath.safeParse({ path: "/w", trust_workspace_config: true }).success,
  ).toBe(false);
  expect(ipcArgs.workspacePath.safeParse({ path: "" }).success).toBe(false);
  expect(ipcArgs.workspaceTrust.safeParse({ path: "/w", decision: "maybe" }).success).toBe(false);
  expect(ipcArgs.keyForSession.safeParse({ key: "   " }).success).toBe(false);
  expect(ipcArgs.externalOpen.safeParse({ url: `https://a/${"x".repeat(2100)}` }).success).toBe(
    false,
  );
  // Preferences drop keys they do not know instead of failing.
  expect(ipcArgs.prefsSet.parse({ theme: "dark", evil: 1 })).toEqual({ theme: "dark" });
});
