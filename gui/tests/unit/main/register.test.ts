// @vitest-environment node
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import { ExternalLinks } from "../../../src/main/external/links";
import { KnownWorkspaces } from "../../../src/main/ipc/known";
import { registerIpc } from "../../../src/main/ipc/register";
import { LiveBuffers } from "../../../src/main/live/buffer";
import { RpcError } from "../../../src/main/sidecar/client";
import { TrustStore } from "../../../src/main/trust/store";
import type { SidecarState } from "../../../src/shared/api";
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
  profile: null as { displayName: string } | null,
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
        command_runner: { kind: "host", image: null, network: null, problem: null },
      };
    }
    return {};
  });
  const stateListeners = new Set<(state: SidecarState) => void>();
  const supervisor = {
    request: request as never,
    restart: vi.fn(),
    setOpenWorkspaces: vi.fn(),
    state: { state: "ready", reopened: [] } as SidecarState,
    onState: (listener: (state: SidecarState) => void) => {
      stateListeners.add(listener);
      return () => stateListeners.delete(listener);
    },
  };
  const emit = (state: SidecarState) => {
    supervisor.state = state;
    for (const listener of [...stateListeners]) listener(state);
  };
  let keyState: { present: boolean; source: "env" | "session" | "login_shell" | "none" } = {
    present: true,
    source: "env",
  };
  const keys = {
    status: vi.fn(() => keyState),
    resolve: vi.fn(async () => keyState),
    setForSession: vi.fn(() => ({ present: true, source: "session" as const })),
    clearSession: vi.fn(() => {
      keyState = { present: false, source: "none" };
      return keyState;
    }),
  };
  const setKey = (source: "env" | "session" | "login_shell" | "none") => {
    keyState = { present: source !== "none", source };
  };
  const recorded: string[] = [];
  const known = new KnownWorkspaces();
  const live = new LiveBuffers();
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
    live,
    links: new ExternalLinks({ confirm, open }),
    appInfo: { name: "App", version: "0.1.0", platform: "darwin" },
    trustStore: new TrustStore(join(tempDir("reg-"), "trust.json")),
    prefs: {
      get: async () => ({ ...basePrefs }),
      set: async (patch) => ({ ...basePrefs, ...patch }),
      setDisplayName: async (name) => ({
        ...basePrefs,
        profile: name === "" ? null : { displayName: name },
      }),
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
    emit,
    setKey,
    supervisor,
    keys,
    recorded,
    request,
    known,
    live,
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
      value: {
        workspace: "/picked",
        trust: "none",
        commandRunner: { kind: "host", image: null, network: null, problem: null },
      },
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

describe("the profile", () => {
  test("a valid name is trimmed and stored, and an empty one clears it", async () => {
    const t = setup();
    expect(await t.call(channels.profileSetDisplayName, { displayName: "  Ada  " })).toEqual({
      ok: true,
      value: { ...basePrefs, profile: { displayName: "Ada" } },
    });
    expect(await t.call(channels.profileSetDisplayName, { displayName: "   " })).toEqual({
      ok: true,
      value: { ...basePrefs, profile: null },
    });
  });

  test("an invalid request is refused: long, control characters, wrong type, extra keys, no body", async () => {
    const t = setup();
    for (const bad of [
      { displayName: "x".repeat(61) },
      { displayName: "a\nb" },
      { displayName: "a\u0000b" },
      { displayName: 5 },
      { displayName: "Ada", role: "admin" },
      { displayName: "Ada", apiKey: "k" },
      {},
      undefined,
    ]) {
      expect(await t.call(channels.profileSetDisplayName, bad)).toMatchObject({
        ok: false,
        error: { code: "invalid_arguments" },
      });
    }
  });

  test("a request from a frame that is not the app's is refused", async () => {
    const t = setup();
    expect(
      await t.call(
        channels.profileSetDisplayName,
        { displayName: "Ada" },
        { url: "https://evil.example/" },
      ),
    ).toMatchObject({ ok: false });
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

describe("signing out", () => {
  const running = {
    workspaces: [{ workspace: "/w", provider: "deepseek", active_turn: { turn_id: "T" } }],
  };
  const idle = { workspaces: [{ workspace: "/w", provider: "deepseek", active_turn: null }] };

  test("a typed key is forgotten, the sidecar restarts, and the reply is true only once it waits for a key", async () => {
    const t = setup({ request: () => idle });
    t.setKey("session");
    t.supervisor.restart.mockImplementation(() => {
      t.emit({ state: "starting" });
      setTimeout(() => t.emit({ state: "needs_key" }), 5);
    });
    const reply = await t.call(channels.keySignOut);
    expect(reply).toEqual({ ok: true, value: { present: false, source: "none" } });
    expect(t.keys.clearSession).toHaveBeenCalledTimes(1);
    expect(t.supervisor.restart).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(reply)).not.toMatch(/sk-/);
  });

  test("main refuses while any turn runs, and neither the key nor the sidecar is touched", async () => {
    const t = setup({ request: () => running });
    t.setKey("session");
    expect(await t.call(channels.keySignOut)).toMatchObject({
      ok: false,
      error: { code: "turn_running" },
    });
    expect(t.keys.clearSession).not.toHaveBeenCalled();
    expect(t.supervisor.restart).not.toHaveBeenCalled();
  });

  test("when the turn check itself fails, nothing is signed out", async () => {
    const t = setup({
      request: () => {
        throw new Error("boom");
      },
    });
    t.setKey("session");
    expect(await t.call(channels.keySignOut)).toMatchObject({
      ok: false,
      error: { code: "turn_check_failed" },
    });
    expect(t.keys.clearSession).not.toHaveBeenCalled();
  });

  test("a key from the environment or the login shell, or no key, is refused", async () => {
    for (const [source, code] of [
      ["env", "key_not_removable"],
      ["login_shell", "key_not_removable"],
      ["none", "not_signed_in"],
    ] as const) {
      const t = setup({ request: () => idle });
      t.setKey(source);
      expect(await t.call(channels.keySignOut)).toMatchObject({ ok: false, error: { code } });
      expect(t.keys.clearSession).not.toHaveBeenCalled();
      expect(t.supervisor.restart).not.toHaveBeenCalled();
    }
  });

  test("a sidecar that cannot be reached counts as no turn, so sign-out goes ahead", async () => {
    const { RpcError } = await import("../../../src/main/sidecar/client");
    const t = setup({
      request: () => {
        throw new RpcError("sidecar_unavailable", "the sidecar is not running");
      },
    });
    t.setKey("session");
    t.supervisor.restart.mockImplementation(() =>
      setTimeout(() => t.emit({ state: "needs_key" }), 1),
    );
    expect(await t.call(channels.keySignOut)).toMatchObject({ ok: true });
  });

  test("a restart that fails is reported as a failure, after the key is already gone", async () => {
    const t = setup({ request: () => idle });
    t.setKey("session");
    t.supervisor.restart.mockImplementation(() =>
      setTimeout(
        () => t.emit({ state: "failed", reason: "no_ready", message: "did not say ready" }),
        1,
      ),
    );
    expect(await t.call(channels.keySignOut)).toMatchObject({
      ok: false,
      error: { code: "restart_failed" },
    });
    expect(t.keys.clearSession).toHaveBeenCalledTimes(1);
  });

  test("a restart that never settles times out instead of claiming success", async () => {
    vi.useFakeTimers();
    try {
      const t = setup({ request: () => idle });
      t.setKey("session");
      const pending = t.call(channels.keySignOut);
      await vi.advanceTimersByTimeAsync(20_000);
      expect(await pending).toMatchObject({ ok: false, error: { code: "restart_timeout" } });
    } finally {
      vi.useRealTimers();
    }
  });

  test("the renderer cannot steer it: arguments never reach the handler", async () => {
    const t = setup({ request: () => idle });
    t.setKey("session");
    t.supervisor.restart.mockImplementation(() =>
      setTimeout(() => t.emit({ state: "needs_key" }), 1),
    );
    expect(await t.call(channels.keySignOut, { key: "sk-x", force: true })).toMatchObject({
      ok: true,
    });
    expect(t.keys.clearSession).toHaveBeenCalledWith();
    expect(t.request).not.toHaveBeenCalledWith(
      "workspace.list",
      expect.objectContaining({ force: true }),
    );
  });
});

describe("resilience channels", () => {
  const created = (workspace: string, session: string, turn: string) => ({
    kind: "event" as const,
    workspace,
    session_id: session,
    turn_id: turn,
    event: { id: `c-${turn}`, type: "turn.created", session_id: session, turn_id: turn },
  });

  test("live.attach returns the session's buffered frames and its running turn", async () => {
    const t = setup();
    t.live.push(created("/w", "s1", "T1") as never);
    expect(await t.call(channels.liveAttach, { workspace: "/w", sessionId: "s1" })).toEqual({
      ok: true,
      value: { frames: [created("/w", "s1", "T1")], activeTurnId: "T1" },
    });
    expect(await t.call(channels.liveAttach, { workspace: "/w", sessionId: "other" })).toEqual({
      ok: true,
      value: { frames: [], activeTurnId: null },
    });
  });

  test("live.attach takes exactly a workspace and a session id", async () => {
    const t = setup();
    for (const bad of [{}, { workspace: "/w" }, { workspace: "/w", sessionId: "s", extra: 1 }]) {
      expect(await t.call(channels.liveAttach, bad)).toMatchObject({
        ok: false,
        error: { code: "invalid_arguments" },
      });
    }
  });

  test("closing a workspace empties its buffer", async () => {
    const t = setup();
    await t.call(channels.workspacePick);
    await t.call(channels.workspaceOpen, { path: "/picked" });
    t.live.push(created("/picked", "s1", "T1") as never);
    await t.call(channels.workspaceClose, { path: "/picked" });
    expect(t.live.attach("/picked", "s1").frames).toEqual([]);
  });

  test("turn.start marks the workspace running, and a refused one takes the mark back", async () => {
    const t = setup();
    const params = { workspace: "/w", session_id: "s1", input: "go" };
    await t.call(channels.rpcCall, { method: "turn.start", params });
    expect(t.live.running).toBe(true);
    t.live.clear();
    t.request.mockRejectedValueOnce(new RpcError("turn_in_progress", "busy"));
    await t.call(channels.rpcCall, { method: "turn.start", params });
    expect(t.live.running).toBe(false);
  });

  test("a turn that starts and ends before the reply is handled does not stay running", async () => {
    const t = setup({
      request: (method) => {
        if (method === "turn.start") {
          // The frames of a fast turn are handled before the reply's continuation runs.
          const event = (type: string, extra: Record<string, unknown>) => ({
            kind: "event" as const,
            workspace: "/w",
            session_id: "s1",
            turn_id: "T1",
            event: { id: `x-${type}`, type, ...extra },
          });
          t.live.push(event("turn.created", { session_id: "s1", turn_id: "T1" }) as never);
          t.live.push(event("turn.done", { state: "done" }) as never);
        }
        return {};
      },
    });
    await t.call(channels.rpcCall, {
      method: "turn.start",
      params: { workspace: "/w", session_id: "s1", input: "go" },
    });
    expect(t.live.running).toBe(false);
  });

  test("sidecar.state reports the supervisor's state", async () => {
    const t = setup();
    expect(await t.call(channels.sidecarState)).toEqual({
      ok: true,
      value: { state: "ready", reopened: [] },
    });
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
