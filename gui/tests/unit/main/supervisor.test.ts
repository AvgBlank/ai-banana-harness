// @vitest-environment node
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { RpcError } from "../../../src/main/sidecar/client";
import { sidecarEnv } from "../../../src/main/sidecar/env";
import { type SidecarState, SidecarSupervisor } from "../../../src/main/sidecar/supervisor";
import { FakeChild } from "./fake-child";

const KEY = "sk-test-0123456789abcdef";

interface Harness {
  supervisor: SidecarSupervisor;
  children: FakeChild[];
  spawned: Array<{ bin: string; args: string[]; env: Record<string, string> }>;
  states: SidecarState[];
  logs: string[];
  reopen: ReturnType<typeof vi.fn>;
}

function setup(): Harness {
  const children: FakeChild[] = [];
  const spawned: Harness["spawned"] = [];
  const logs: string[] = [];
  const reopen = vi.fn(async () => undefined);
  const supervisor = new SidecarSupervisor({
    bin: "/bin/sidecar",
    spawn: (bin, args, options) => {
      const child = new FakeChild();
      children.push(child);
      spawned.push({ bin, args, env: options.env });
      return child;
    },
    environment: () => ({
      variables: sidecarEnv(
        { HOME: "/h", NODE_OPTIONS: "--inspect", ELECTRON_RUN_AS_NODE: "1" },
        { apiKey: KEY },
      ),
      apiKey: KEY,
    }),
    reopenWorkspace: reopen,
    log: (message) => logs.push(message),
  });
  const states: SidecarState[] = [];
  supervisor.onState((state) => states.push(state));
  return { supervisor, children, spawned, states, logs, reopen };
}

const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

async function started(h: Harness): Promise<FakeChild> {
  h.supervisor.start();
  const child = h.children[h.children.length - 1] as FakeChild;
  child.ready();
  await advance(0);
  return child;
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("handshake", () => {
  test("ready within 10 s moves to ready; the child gets no key in argv and an explicit env", async () => {
    const h = setup();
    h.supervisor.start();
    expect(h.supervisor.state).toEqual({ state: "starting" });
    h.children[0]?.ready();
    await advance(0);
    expect(h.supervisor.state).toEqual({ state: "ready", reopened: [] });
    expect(h.spawned[0]?.args).toEqual(["serve", "--stdio"]);
    expect(JSON.stringify(h.spawned[0]?.args)).not.toContain(KEY);
    expect(h.spawned[0]?.env).toEqual({ HOME: "/h", AI_API_KEY: KEY });
  });

  test("no ready in 10 s fails with no_ready, kills the child and does not restart", async () => {
    const h = setup();
    h.supervisor.start();
    await advance(10_000);
    expect(h.supervisor.state).toMatchObject({ state: "failed", reason: "no_ready" });
    expect(h.children[0]?.signals).toContain("SIGKILL");
    await advance(60_000);
    expect(h.children).toHaveLength(1);
  });

  test("a different protocol fails with incompatible_protocol and no restart", async () => {
    const h = setup();
    h.supervisor.start();
    h.children[0]?.ready({ protocol: 2 });
    await advance(5_000);
    expect(h.supervisor.state).toMatchObject({ state: "failed", reason: "incompatible_protocol" });
    expect(h.children[0]?.signals).toContain("SIGKILL");
    expect(h.children).toHaveLength(1);
  });

  test("a fatal frame fails with fatal and its message; the exit that follows changes nothing", async () => {
    const h = setup();
    h.supervisor.start();
    h.children[0]?.frame({
      kind: "fatal",
      error: { code: "startup_failed", message: `cannot open ${KEY}` },
    });
    await advance(0);
    h.children[0]?.exit(1);
    await advance(10_000);
    expect(h.supervisor.state).toEqual({
      state: "failed",
      reason: "fatal",
      message: "cannot open [REDACTED]",
    });
    expect(h.children).toHaveLength(1);
  });

  test("an early exit fails with exited_early and the last 4 KiB of stderr, redacted", async () => {
    const h = setup();
    h.supervisor.start();
    h.children[0]?.stderr.write(`${"x".repeat(6000)} key=${KEY}`);
    await advance(0);
    h.children[0]?.exit(1);
    await advance(0);
    const state = h.supervisor.state;
    expect(state).toMatchObject({ state: "failed", reason: "exited_early" });
    const message = (state as { message: string }).message;
    expect(message.length).toBeLessThanOrEqual(4096 + 10);
    expect(message).toContain("[REDACTED]");
    expect(message).not.toContain(KEY);
    expect(h.children).toHaveLength(1);
  });

  test("api_key_present false is needs_key, and the sidecar still answers requests", async () => {
    const h = setup();
    h.supervisor.start();
    const child = h.children[0] as FakeChild;
    child.respond = () => ({ sessions: [] });
    child.ready({ api_key_present: false });
    await advance(0);
    expect(h.supervisor.state).toEqual({ state: "needs_key" });
    const pending = h.supervisor.request("session.list", {});
    await advance(0);
    await expect(pending).resolves.toEqual({ sessions: [] });
  });

  test("a frame before ready is a corrupted stream", async () => {
    const h = setup();
    h.supervisor.start();
    h.children[0]?.raw("not json\n");
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "crashed", attempt: 1 });
    expect(h.children[0]?.signals).toContain("SIGKILL");
  });
});

describe("framing", () => {
  test("oversized, non-UTF-8 and non-JSON lines are dropped and counted; frames after them still work", async () => {
    const h = setup();
    const child = await started(h);
    child.respond = () => ({ sessions: [] });
    child.raw(Buffer.concat([Buffer.alloc(8 * 1024 * 1024 + 5, 0x61), Buffer.from("\n")]));
    child.raw(Buffer.from([0xff, 0xfe, 0x0a]));
    child.raw("{not json}\n");
    child.raw('{"kind":"event"}\n');
    await advance(0);
    expect(h.supervisor.droppedLines).toBe(4);
    expect(h.supervisor.state).toMatchObject({ state: "ready" });
    const reply = h.supervisor.request("session.list", {});
    await advance(0);
    await expect(reply).resolves.toEqual({ sessions: [] });
    expect(h.logs.join("\n")).not.toContain("not json");
  });

  test("20 consecutive invalid lines kill and restart; a valid frame resets the count", async () => {
    const h = setup();
    const child = await started(h);
    child.raw("x\n".repeat(19));
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "ready" });
    child.frame({ kind: "response", id: "unknown", ok: true, result: {} });
    child.raw("x\n".repeat(19));
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "ready" });
    child.raw("x\n");
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "crashed" });
    expect(child.signals).toContain("SIGKILL");
  });

  test("ids are generated here, unknown ids are ignored, and a late response after a timeout is ignored", async () => {
    const h = setup();
    const child = await started(h);
    const first = h.supervisor.request("session.list", {});
    const second = h.supervisor.request("workspace.list", {});
    await advance(0);
    expect(child.received.map((r) => r.id)).toEqual(["m1", "m2"]);

    child.frame({ kind: "response", id: "m99", ok: true, result: {} });
    child.frame({ kind: "response", id: "m2", ok: true, result: { workspaces: [] } });
    await advance(0);
    await expect(second).resolves.toEqual({ workspaces: [] });

    first.catch(() => undefined);
    await advance(30_000);
    await expect(first).rejects.toMatchObject({ code: "timeout" });
    // The late answer finds nobody waiting.
    child.frame({ kind: "response", id: "m1", ok: true, result: { sessions: [] } });
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "ready" });
    expect(h.supervisor.droppedLines).toBe(0);
  });

  test("open, resume, undo and changes wait 60 s; others 30 s", async () => {
    const h = setup();
    await started(h);
    const fast = h.supervisor.request("session.list", {});
    const slow = h.supervisor.request("changes.workspace", { workspace: "/w" });
    const open = h.supervisor.request("workspace.open", { path: "/w" });
    fast.catch(() => undefined);
    slow.catch(() => undefined);
    open.catch(() => undefined);
    await advance(30_000);
    await expect(fast).rejects.toMatchObject({ code: "timeout" });
    const state = vi.fn();
    slow.then(state, state);
    open.then(state, state);
    await advance(0);
    expect(state).not.toHaveBeenCalled();
    await advance(30_000);
    await expect(slow).rejects.toMatchObject({ code: "timeout" });
    await expect(open).rejects.toMatchObject({ code: "timeout" });
  });

  test("a sidecar error response and an unexpected result surface as RpcError, redacted", async () => {
    const h = setup();
    const child = await started(h);
    const bad = h.supervisor.request("session.list", {});
    const wrong = h.supervisor.request("workspace.list", {});
    await advance(0);
    child.frame({
      kind: "response",
      id: "m1",
      ok: false,
      error: { code: "internal", message: `oops ${KEY}` },
    });
    child.frame({ kind: "response", id: "m2", ok: true, result: { workspaces: "no" } });
    const error = await bad.catch((e: RpcError) => e);
    expect(error).toMatchObject({ code: "internal", message: "oops [REDACTED]" });
    await expect(wrong).rejects.toMatchObject({ code: "invalid_result" });
  });

  test("stdin writes wait for drain and keep their order", async () => {
    const h = setup();
    const child = await started(h);
    let pushedBack = true;
    const written: string[] = [];
    const drains: Array<() => void> = [];
    child.stdin.write = ((chunk: string, callback?: (e?: Error | null) => void) => {
      written.push(chunk.trim());
      callback?.();
      return !pushedBack;
    }) as never;
    child.stdin.once = ((event: string, listener: () => void) => {
      if (event === "drain") drains.push(listener);
      return child.stdin;
    }) as never;
    h.supervisor.request("session.list", {}).catch(() => undefined);
    h.supervisor.request("workspace.list", {}).catch(() => undefined);
    await advance(0);
    expect(written).toHaveLength(1);
    pushedBack = false;
    drains.shift()?.();
    await advance(0);
    expect(written).toHaveLength(2);
    expect(written.map((line) => JSON.parse(line).id)).toEqual(["m1", "m2"]);
  });
});

describe("crash handling", () => {
  test("pending requests reject with sidecar_exited, then restarts at 0.5, 1 and 2 s, then failed", async () => {
    const h = setup();
    const first = await started(h);
    const pending = h.supervisor.request("session.list", {});
    pending.catch(() => undefined);
    first.exit(1);
    await advance(0);
    await expect(pending).rejects.toMatchObject({ code: "sidecar_exited" });
    expect(h.supervisor.state).toEqual({ state: "crashed", attempt: 1, restartInMs: 500 });
    await expect(h.supervisor.request("session.list", {})).rejects.toMatchObject({
      code: "sidecar_unavailable",
    });

    await advance(499);
    expect(h.children).toHaveLength(1);
    await advance(1);
    expect(h.children).toHaveLength(2);
    h.children[1]?.ready();
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "ready" });

    h.children[1]?.exit(1);
    await advance(0);
    expect(h.supervisor.state).toEqual({ state: "crashed", attempt: 2, restartInMs: 1000 });
    await advance(1000);
    h.children[2]?.ready();
    await advance(0);
    h.children[2]?.exit(1);
    await advance(0);
    expect(h.supervisor.state).toEqual({ state: "crashed", attempt: 3, restartInMs: 2000 });
    await advance(2000);
    h.children[3]?.ready();
    await advance(0);
    h.children[3]?.exit(1);
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ state: "failed", reason: "restart_budget" });
    await advance(120_000);
    expect(h.children).toHaveLength(4);

    // The user's Restart gives a fresh budget and a fresh process.
    h.supervisor.restart();
    expect(h.children).toHaveLength(5);
  });

  test("60 s of stable running resets the budget", async () => {
    const h = setup();
    const first = await started(h);
    first.exit(1);
    await advance(500);
    h.children[1]?.ready();
    await advance(0);
    h.children[1]?.exit(1);
    await advance(0);
    expect(h.supervisor.state).toMatchObject({ attempt: 2 });
    await advance(1000);
    h.children[2]?.ready();
    await advance(60_000);
    h.children[2]?.exit(1);
    await advance(0);
    expect(h.supervisor.state).toEqual({ state: "crashed", attempt: 1, restartInMs: 500 });
  });

  test("after a restart the open workspaces are reopened one at a time, never on first start", async () => {
    const h = setup();
    const order: string[] = [];
    const gates: Array<() => void> = [];
    h.reopen.mockImplementation(
      (workspace: string) =>
        new Promise<void>((resolve) => {
          order.push(`open ${workspace}`);
          gates.push(() => {
            order.push(`done ${workspace}`);
            resolve();
          });
        }),
    );
    const first = await started(h);
    expect(h.reopen).not.toHaveBeenCalled();
    h.supervisor.setOpenWorkspaces(["/a", "/b"]);
    first.exit(1);
    await advance(500);
    h.children[1]?.ready();
    await advance(0);
    expect(order).toEqual(["open /a"]);
    expect(h.supervisor.state).toEqual({ state: "starting" });
    gates.shift()?.();
    await advance(0);
    expect(order).toEqual(["open /a", "done /a", "open /b"]);
    gates.shift()?.();
    await advance(0);
    expect(h.supervisor.state).toEqual({ state: "ready", reopened: ["/a", "/b"] });
  });
});

describe("shutdown", () => {
  test("a clean sidecar: shutdown request, stdin closed, exit, stopped", async () => {
    const h = setup();
    const child = await started(h);
    child.respond = (request) => (request.method === "shutdown" ? {} : undefined);
    child.stdin.on("end", () => queueMicrotask(() => child.exit(0)));
    const done = h.supervisor.shutdown();
    await advance(0);
    expect(child.received.map((r) => r.method)).toEqual(["shutdown"]);
    await done;
    expect(child.ended).toBe(true);
    expect(child.signals).toEqual([]);
    expect(h.supervisor.state).toEqual({ state: "stopped" });
    await advance(120_000);
    expect(h.children).toHaveLength(1);
  });

  test("a stuck sidecar is escalated: 4 s, close stdin, 5 s, SIGTERM, 2 s, SIGKILL", async () => {
    const h = setup();
    const child = await started(h);
    const done = h.supervisor.shutdown();
    await advance(3_999);
    expect(child.ended).toBe(false);
    await advance(1);
    expect(child.ended).toBe(true);
    await advance(4_999);
    expect(child.signals).toEqual([]);
    await advance(1);
    expect(child.signals).toEqual(["SIGTERM"]);
    await advance(1_999);
    expect(child.signals).toEqual(["SIGTERM"]);
    await advance(1);
    expect(child.signals).toEqual(["SIGTERM", "SIGKILL"]);
    child.exit(137);
    await done;
    expect(h.supervisor.state).toEqual({ state: "stopped" });
  });

  test("a second quit forces the kill path", async () => {
    const h = setup();
    const child = await started(h);
    const first = h.supervisor.shutdown();
    await advance(10);
    const forced = h.supervisor.shutdown(true);
    await advance(2_000);
    expect(child.signals).toEqual(["SIGTERM", "SIGKILL"]);
    child.exit(137);
    await forced;
    await first;
  });

  test("a crash while stopping is not restarted", async () => {
    const h = setup();
    const child = await started(h);
    const done = h.supervisor.shutdown();
    child.exit(1);
    await done;
    await advance(60_000);
    expect(h.children).toHaveLength(1);
  });
});

describe("secrets", () => {
  test("the key appears only in the spawn environment, never in argv or in logs", async () => {
    const h = setup();
    const child = await started(h);
    child.raw(`${KEY}\n`);
    child.stderr.write(KEY);
    await advance(0);
    expect(JSON.stringify(h.spawned.map((s) => s.args))).not.toContain(KEY);
    expect(h.logs.join("\n")).not.toContain(KEY);
    expect(Object.entries(h.spawned[0]?.env ?? {}).filter(([, v]) => v === KEY)).toEqual([
      ["AI_API_KEY", KEY],
    ]);
  });
});
