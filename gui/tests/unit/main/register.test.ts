// @vitest-environment node
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import { ExternalLinks } from "../../../src/main/external/links";
import { KnownWorkspaces } from "../../../src/main/ipc/known";
import { registerIpc } from "../../../src/main/ipc/register";
import { RpcError } from "../../../src/main/sidecar/client";
import type { SidecarState } from "../../../src/main/sidecar/supervisor";
import { TrustStore } from "../../../src/main/trust/store";
import { channels } from "../../../src/shared/channels";
import { tempDir } from "./tmp";

const basePrefs = {
  theme: "dark" as const,
  sidebarOpen: true,
  changesOpen: false,
  changesScope: "turn" as const,
  changesWidth: 480,
  lastWorkspace: null,
  recents: [] as string[],
};

type Handler = (
  event: { senderFrame: { url: string } | null },
  arg?: unknown,
) => Promise<{
  ok: boolean;
  value?: unknown;
  error?: { code: string; message: string };
}>;

function setup(options: { request?: (method: string, params: unknown) => unknown } = {}) {
  const handlers = new Map<string, Handler>();
  const topFrame = { url: "app://banana/index.html" };
  const request = vi.fn(async (method: string, params: unknown) => {
    if (options.request) return options.request(method, params);
    if (method === "workspace.open") {
      return {
        workspace: (params as { path: string }).path,
        already_open: false,
        provider: "deepseek",
        workspace_config: { present: false, loaded: false, path: "p" },
      };
    }
    return {};
  });
  const supervisor = {
    request: request as never,
    restart: vi.fn(),
    setOpenWorkspaces: vi.fn(),
    state: { state: "ready", reopened: [] } as SidecarState,
  };
  const keyState = { present: true, source: "env" as const };
  const keys = {
    status: vi.fn(() => keyState),
    resolve: vi.fn(async () => keyState),
    setForSession: vi.fn(() => ({ present: true, source: "session" as const })),
  };
  const recorded: string[] = [];
  const known = new KnownWorkspaces();
  const logs: string[] = [];
  const confirm = vi.fn(async () => ({ open: true, trustHost: false }));
  const open = vi.fn(async () => undefined);
  let picked: string | null = "/picked";
  registerIpc({
    ipc: { handle: (channel, listener) => handlers.set(channel, listener as Handler) },
    trust: { topFrame: () => topFrame, devServer: undefined, log: (m) => logs.push(m) },
    supervisor,
    keys,
    known,
    links: new ExternalLinks({ confirm, open }),
    appInfo: { name: "App", version: "0.1.0", platform: "darwin" },
    trustStore: new TrustStore(join(tempDir("reg-"), "trust.json")),
    prefs: {
      get: async () => ({ ...basePrefs }),
      set: async (patch) => ({ ...basePrefs, ...patch }),
      recordWorkspace: async (workspace) => {
        recorded.push(workspace);
        return { ...basePrefs, lastWorkspace: workspace };
      },
    },
    pickFolder: async () => picked,
  });
  const call = (channel: string, arg?: unknown, frame: { url: string } | null = topFrame) =>
    (handlers.get(channel) as Handler)({ senderFrame: frame }, arg);
  return {
    call,
    supervisor,
    keys,
    recorded,
    request,
    known,
    logs,
    confirm,
    open,
    pick: (p: string | null) => (picked = p),
    topFrame,
  };
}

describe("sender checks", () => {
  test("every channel refuses another frame, a null frame and a foreign origin", async () => {
    const t = setup();
    for (const channel of Object.values(channels)) {
      for (const frame of [
        { url: "app://banana/index.html" },
        null,
        { url: "https://evil.example/" },
      ]) {
        const result = await t.call(channel, {}, frame);
        expect(result, `${channel} ${frame?.url}`).toMatchObject({
          ok: false,
          error: { code: "untrusted_sender" },
        });
      }
    }
    expect(t.request).not.toHaveBeenCalled();
    expect(t.logs.length).toBeGreaterThan(0);
  });

  test("the top frame on the app origin is trusted", async () => {
    const t = setup();
    expect(await t.call(channels.appInfo)).toEqual({
      ok: true,
      value: { name: "App", version: "0.1.0", platform: "darwin", protocol: 1 },
    });
  });
});

describe("rpc.call", () => {
  test("forwards allowlisted methods and returns the result", async () => {
    const t = setup({ request: () => ({ workspaces: [] }) });
    expect(await t.call(channels.rpcCall, { method: "workspace.list", params: {} })).toEqual({
      ok: true,
      value: { workspaces: [] },
    });
    expect(t.request).toHaveBeenCalledWith("workspace.list", {});
  });

  test("refuses shutdown, workspace.open, workspace.close and unknown methods with method_not_allowed", async () => {
    const t = setup();
    for (const method of ["shutdown", "workspace.open", "workspace.close", "nope.nope"]) {
      expect(
        await t.call(channels.rpcCall, { method, params: { path: "/x" } }),
        method,
      ).toMatchObject({
        ok: false,
        error: { code: "method_not_allowed" },
      });
    }
    expect(t.request).not.toHaveBeenCalled();
  });

  test("rejects extra keys, a smuggled trust flag and over-long strings before the sidecar sees them", async () => {
    const t = setup();
    const bad = [
      { method: "session.create", params: { workspace: "/w", trust_workspace_config: true } },
      { method: "session.create", params: { workspace: "x".repeat(5000) } },
      { method: "session.list", params: {}, trust_workspace_config: true },
    ];
    for (const arg of bad) {
      expect(await t.call(channels.rpcCall, arg)).toMatchObject({
        ok: false,
        error: { code: "invalid_arguments" },
      });
    }
    expect(t.request).not.toHaveBeenCalled();
  });

  test("sidecar and unexpected errors leave as codes without stacks or internals", async () => {
    const failing = setup({
      request: () => {
        throw new RpcError("timeout", "session.list timed out");
      },
    });
    expect(await failing.call(channels.rpcCall, { method: "session.list", params: {} })).toEqual({
      ok: false,
      error: { code: "timeout", message: "session.list timed out" },
    });
    const broken = setup({
      request: () => {
        throw new Error("/secret/path exploded sk-test-0123456789abcdef");
      },
    });
    const result = await broken.call(channels.rpcCall, { method: "session.list", params: {} });
    expect(result).toEqual({
      ok: false,
      error: { code: "internal", message: "the request failed" },
    });
  });
});

describe("workspaces", () => {
  test("a path main does not know is unknown_workspace and never reaches the sidecar", async () => {
    const t = setup();
    expect(await t.call(channels.workspaceOpen, { path: "/etc" })).toMatchObject({
      ok: false,
      error: { code: "unknown_workspace" },
    });
    expect(t.request).not.toHaveBeenCalled();
  });

  test("a picked folder opens, always restricted, and the renderer cannot add a trust flag", async () => {
    const t = setup();
    expect(await t.call(channels.workspacePick)).toEqual({ ok: true, value: "/picked" });
    expect(
      await t.call(channels.workspaceOpen, { path: "/picked", trust_workspace_config: true }),
    ).toMatchObject({
      ok: false,
      error: { code: "invalid_arguments" },
    });
    expect(await t.call(channels.workspaceOpen, { path: "/picked" })).toEqual({
      ok: true,
      value: { workspace: "/picked", trust: "none" },
    });
    expect(t.request).toHaveBeenCalledWith("workspace.open", {
      path: "/picked",
      trust_workspace_config: false,
    });
  });

  test("a cancelled dialog teaches main nothing", async () => {
    const t = setup();
    t.pick(null);
    expect(await t.call(channels.workspacePick)).toEqual({ ok: true, value: null });
    expect(t.known.has("/picked")).toBe(false);
  });

  test("the workspaces in a session list become known", async () => {
    const t = setup({ request: () => ({ sessions: [{ workspace: "/from/db" }] }) });
    await t.call(channels.rpcCall, { method: "session.list", params: {} });
    expect(t.known.has("/from/db")).toBe(true);
  });
});

describe("external links", () => {
  test("a bad scheme is refused without a dialog; a good link is confirmed then opened", async () => {
    const t = setup();
    expect(await t.call(channels.externalOpen, { url: "file:///etc/passwd" })).toMatchObject({
      ok: false,
      error: { code: "invalid_url" },
    });
    expect(t.confirm).not.toHaveBeenCalled();
    expect(await t.call(channels.externalOpen, { url: "https://example.com/x" })).toEqual({
      ok: true,
    });
    expect(t.confirm).toHaveBeenCalledTimes(1);
    expect(t.open).toHaveBeenCalledWith("https://example.com/x");
  });
});

describe("preferences", () => {
  test("get and set go through the store, and a patch is validated with unknown keys dropped", async () => {
    const t = setup();
    expect(await t.call(channels.prefsGet)).toEqual({ ok: true, value: basePrefs });
    expect(
      await t.call(channels.prefsSet, { theme: "light", evil: 1, lastWorkspace: "/x" }),
    ).toEqual({
      ok: true,
      value: { ...basePrefs, theme: "light" },
    });
    expect(await t.call(channels.prefsSet, { theme: "purple" })).toMatchObject({
      ok: false,
      error: { code: "invalid_arguments" },
    });
    expect(await t.call(channels.prefsSet, { sidebarOpen: "yes" })).toMatchObject({
      ok: false,
      error: { code: "invalid_arguments" },
    });
  });
});

describe("the API key", () => {
  test("status carries only presence and source", async () => {
    const t = setup();
    expect(await t.call(channels.keyStatus)).toEqual({
      ok: true,
      value: { present: true, source: "env" },
    });
  });

  test("a typed key is stored by the resolver, the sidecar restarts, and the reply is the status alone", async () => {
    const t = setup();
    const reply = await t.call(channels.keySetForSession, { key: "  sk-test-0123456789abcdef  " });
    expect(reply).toEqual({ ok: true, value: { present: true, source: "session" } });
    expect(t.keys.setForSession).toHaveBeenCalledWith("sk-test-0123456789abcdef");
    expect(t.supervisor.restart).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(reply)).not.toContain("sk-test");
  });

  test("empty, oversized and extra-key submissions are refused", async () => {
    const t = setup();
    for (const arg of [{ key: "   " }, { key: "k".repeat(513) }, { key: "k", extra: 1 }, {}]) {
      expect(await t.call(channels.keySetForSession, arg)).toMatchObject({
        ok: false,
        error: { code: "invalid_arguments" },
      });
    }
    expect(t.keys.setForSession).not.toHaveBeenCalled();
    expect(t.supervisor.restart).not.toHaveBeenCalled();
  });

  test("check again restarts the sidecar only when it was waiting for a key and one is found", async () => {
    const waiting = setup();
    waiting.supervisor.state = { state: "needs_key" };
    expect(await waiting.call(channels.keyRecheck)).toMatchObject({ ok: true });
    expect(waiting.keys.resolve).toHaveBeenCalledWith(true);
    expect(waiting.supervisor.restart).toHaveBeenCalledTimes(1);

    const running = setup();
    await running.call(channels.keyRecheck);
    expect(running.supervisor.restart).not.toHaveBeenCalled();
  });
});

describe("trust channels", () => {
  const sha = "a".repeat(64);

  test("review and trust refuse a path main does not know, before the sidecar is asked", async () => {
    const t = setup();
    expect(await t.call(channels.trustReview, { path: "/etc" })).toMatchObject({
      ok: false,
      error: { code: "unknown_workspace" },
    });
    expect(
      await t.call(channels.trustGrant, { path: "/etc", sha256: sha, acknowledgeHosts: true }),
    ).toMatchObject({ ok: false, error: { code: "unknown_workspace" } });
    expect(t.request).not.toHaveBeenCalled();
  });

  test("trusting takes exactly a path, a hash and the acknowledgement", async () => {
    const t = setup();
    await t.call(channels.workspacePick);
    for (const bad of [
      { path: "/picked", sha256: "short", acknowledgeHosts: true },
      { path: "/picked", sha256: sha },
      { path: "/picked", sha256: sha, acknowledgeHosts: true, trust_workspace_config: true },
    ]) {
      expect(await t.call(channels.trustGrant, bad)).toMatchObject({
        ok: false,
        error: { code: "invalid_arguments" },
      });
    }
    expect(t.request).not.toHaveBeenCalled();
  });

  test("the list starts empty", async () => {
    const t = setup();
    expect(await t.call(channels.trustList)).toEqual({ ok: true, value: [] });
  });
});

describe("workspace lifecycle", () => {
  test("opening records the workspace for the recents, and close works only for what is open", async () => {
    const t = setup();
    await t.call(channels.workspacePick);
    await t.call(channels.workspaceOpen, { path: "/picked" });
    expect(t.recorded).toEqual(["/picked"]);
    expect(await t.call(channels.workspaceClose, { path: "/other" })).toMatchObject({
      ok: false,
      error: { code: "unknown_workspace" },
    });
    expect(await t.call(channels.workspaceClose, { path: "/picked" })).toEqual({ ok: true });
    expect(t.request).toHaveBeenLastCalledWith("workspace.close", { workspace: "/picked" });
    expect(t.supervisor.setOpenWorkspaces).toHaveBeenLastCalledWith([]);
    expect(await t.call(channels.workspaceClose, { path: "/picked" })).toMatchObject({
      ok: false,
      error: { code: "unknown_workspace" },
    });
  });

  test("a failed open records nothing", async () => {
    const t = setup({
      request: () => {
        throw new RpcError("api_key_missing", "no key");
      },
    });
    await t.call(channels.workspacePick);
    expect(await t.call(channels.workspaceOpen, { path: "/picked" })).toMatchObject({
      ok: false,
      error: { code: "api_key_missing" },
    });
    expect(t.recorded).toEqual([]);
  });
});
