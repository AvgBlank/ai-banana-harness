// @vitest-environment node
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import { WorkspaceTrust } from "../../../src/main/trust/flow";
import { TrustStore } from "../../../src/main/trust/store";
import type { ConfigFile } from "../../../src/main/trust/summary";
import { tempDir } from "./tmp";

const BASE = [
  { provider: "deepseek", model: "m", host: "api.deepseek.com", scheme: "https" },
  { provider: "qwen", model: "m", host: "dashscope.aliyuncs.com", scheme: "https" },
];
const OVERRIDE = `[providers.deepseek]\nbase_url = "https://proxy.example"\n`;
const sha = (c: string) => c.repeat(64).slice(0, 64);

function setup(
  options: {
    present?: boolean;
    config?: ConfigFile;
    trustedProviders?: typeof BASE;
    activeTurn?: boolean;
    loaded?: boolean;
    openFails?: boolean;
  } = {},
) {
  const store = new TrustStore(join(tempDir("flow-"), "trust.json"));
  const log: string[] = [];
  const opened: Array<[string, boolean]> = [];
  let trusted = false;
  let isOpen = false;
  const request = vi.fn(async (method: string, params: Record<string, unknown>) => {
    log.push(
      method === "workspace.open"
        ? `open:${params.trust_workspace_config ? "trusted" : "restricted"}`
        : method,
    );
    switch (method) {
      case "workspace.open": {
        const already = isOpen;
        if (params.trust_workspace_config) {
          if (options.openFails) throw new Error("bad config");
          trusted = true;
        } else if (!already) trusted = false;
        isOpen = true;
        return {
          workspace: "/w",
          already_open: already,
          provider: "deepseek",
          workspace_config: {
            present: options.present ?? true,
            loaded: trusted || (options.loaded ?? false),
            path: "/w/.banana-harness/config.toml",
          },
        };
      }
      case "workspace.close":
        isOpen = false;
        trusted = false;
        return {};
      case "workspace.list":
        return {
          workspaces: isOpen
            ? [
                {
                  workspace: "/w",
                  provider: "deepseek",
                  active_turn: options.activeTurn
                    ? { session_id: "s", turn_id: null, pending_action_ids: [] }
                    : null,
                },
              ]
            : [],
        };
      case "provider.list":
        return {
          providers: trusted ? (options.trustedProviders ?? BASE) : BASE,
          default: "deepseek",
        };
      default:
        return {};
    }
  });
  const config: ConfigFile = options.config ?? { ok: true, text: OVERRIDE, sha256: sha("a") };
  const flow = new WorkspaceTrust({
    request: request as never,
    store,
    readConfig: async () => config,
    opened: (workspace, open) => opened.push([workspace, open]),
  });
  return { flow, store, log, opened };
}

const code = (promise: Promise<unknown>) =>
  promise.then(
    () => null,
    (e: { code?: string }) => e.code,
  );

describe("opening", () => {
  test("no config means nothing to trust, and nothing is read", async () => {
    const t = setup({ present: false });
    expect(await t.flow.open("/w")).toEqual({ workspace: "/w", trust: "none" });
    expect(t.log).toEqual(["open:restricted"]);
  });

  test("a config nobody trusted stays restricted and asks for review", async () => {
    const t = setup();
    expect(await t.flow.open("/w")).toEqual({ workspace: "/w", trust: "needs_review" });
    expect(t.log).toEqual(["open:restricted"]);
  });

  test("an unreadable config stays restricted", async () => {
    const t = setup({ config: { ok: false, reason: "symlink" } });
    expect(await t.flow.open("/w")).toEqual({ workspace: "/w", trust: "unreadable" });
  });

  test("the same path and hash reopens trusted after checking destinations", async () => {
    const t = setup();
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: ["https://proxy.example"] });
    const opened = await t.flow.open("/w");
    expect(opened.trust).toBe("trusted");
    expect(t.log).toEqual([
      "open:restricted",
      "workspace.list",
      "workspace.list",
      "provider.list",
      "workspace.close",
      "open:trusted",
      "provider.list",
    ]);
  });

  test("a changed file asks again", async () => {
    const t = setup({ config: { ok: true, text: OVERRIDE, sha256: sha("b") } });
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: [] });
    expect((await t.flow.open("/w")).trust).toBe("needs_review");
    expect(t.log).not.toContain("open:trusted");
  });

  test("stored trust is not used while a turn runs", async () => {
    const t = setup({ activeTurn: true });
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: [] });
    expect((await t.flow.open("/w")).trust).toBe("needs_review");
    expect(t.log).not.toContain("workspace.close");
  });

  test("a workspace that is already trusted keeps its config, whatever the file says now", async () => {
    const t = setup({ loaded: true, config: { ok: false, reason: "unreadable" } });
    expect((await t.flow.open("/w")).trust).toBe("trusted");
  });

  test("stored trust whose destinations moved closes the workspace and says blocked", async () => {
    const t = setup({
      trustedProviders: [{ ...BASE[0], host: "evil.example" }, BASE[1]] as typeof BASE,
    });
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: [] });
    expect(await code(t.flow.open("/w"))).toBe("blocked_config");
    expect(t.log.at(-1)).toBe("workspace.close");
    expect(t.opened.at(-1)).toEqual(["/w", false]);
  });

  test("a stored config that fails to load leaves the workspace restricted", async () => {
    const t = setup({ openFails: true });
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: [] });
    expect((await t.flow.open("/w")).trust).toBe("unreadable");
    expect(t.log.at(-1)).toBe("open:restricted");
  });
});

describe("trusting", () => {
  const proxy = [{ ...BASE[0], host: "proxy.example" }, BASE[1]] as typeof BASE;

  test("review carries the raw file, its hash and the baseline", async () => {
    const t = setup();
    const review = await t.flow.review("/w");
    expect(review).toMatchObject({
      workspace: "/w",
      sha256: sha("a"),
      raw: OVERRIDE,
      parseError: null,
    });
    expect(review.baseline).toContainEqual({
      provider: "deepseek",
      origin: "https://api.deepseek.com",
    });
    expect(review.summary?.providers[0]?.differs).toBe(true);
  });

  test("a destination that differs needs the acknowledgement", async () => {
    const t = setup({ trustedProviders: proxy });
    expect(
      await code(t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: false })),
    ).toBe("host_ack_required");
    expect(await t.store.get("/w")).toBeNull();
  });

  test("with the acknowledgement the entry records the approved destination", async () => {
    const t = setup({ trustedProviders: proxy });
    const result = await t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: true });
    expect(result.trust).toBe("trusted");
    expect(await t.store.get("/w")).toEqual({
      configSha256: sha("a"),
      approvedHosts: ["https://proxy.example"],
    });
  });

  test("a file that changed since the review is refused", async () => {
    const t = setup({ trustedProviders: proxy });
    expect(await code(t.flow.trust({ path: "/w", sha256: sha("c"), acknowledgeHosts: true }))).toBe(
      "config_changed",
    );
  });

  test("trusting is refused while a turn runs", async () => {
    const t = setup({ trustedProviders: proxy, activeTurn: true });
    expect(await code(t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: true }))).toBe(
      "turn_running",
    );
    expect(await t.store.get("/w")).toBeNull();
    expect(t.log).not.toContain("workspace.close");
  });

  test("a destination the parser missed is still blocked and nothing is stored", async () => {
    // The summary lists no provider override, but the loaded config sends the key elsewhere.
    const t = setup({
      config: { ok: true, text: "[agent]\ninstructions = 'x'\n", sha256: sha("a") },
      trustedProviders: proxy,
    });
    expect(await code(t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: true }))).toBe(
      "blocked_config",
    );
    expect(await t.store.get("/w")).toBeNull();
    expect(t.opened.at(-1)).toEqual(["/w", false]);
  });

  test("a config that does not parse is refused", async () => {
    const t = setup({ config: { ok: true, text: "a = = 1", sha256: sha("a") } });
    expect(await code(t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: true }))).toBe(
      "config_invalid",
    );
  });

  test("a config that fails to load leaves the workspace restricted", async () => {
    const t = setup({ openFails: true });
    expect(await code(t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: true }))).toBe(
      "config_invalid",
    );
    expect(t.log.at(-1)).toBe("open:restricted");
    expect(await t.store.get("/w")).toBeNull();
  });
});

describe("forgetting", () => {
  test("an open trusted workspace goes back to restricted", async () => {
    const t = setup({
      trustedProviders: [{ ...BASE[0], host: "proxy.example" }, BASE[1]] as typeof BASE,
    });
    await t.flow.trust({ path: "/w", sha256: sha("a"), acknowledgeHosts: true });
    t.log.length = 0;
    expect(await t.flow.forget("/w")).toEqual({ workspace: "/w", trust: "needs_review" });
    expect(await t.store.get("/w")).toBeNull();
    expect(t.log).toContain("workspace.close");
  });

  test("a workspace that is not open is only forgotten", async () => {
    const t = setup();
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: [] });
    expect(await t.flow.forget("/w")).toBeNull();
    expect(await t.store.get("/w")).toBeNull();
  });

  test("a running turn blocks it and the entry stays", async () => {
    const t = setup({ activeTurn: true });
    await t.flow.open("/w");
    await t.store.trust("/w", { configSha256: sha("a"), approvedHosts: [] });
    expect(await code(t.flow.forget("/w"))).toBe("turn_running");
    expect(await t.store.get("/w")).not.toBeNull();
  });
});
