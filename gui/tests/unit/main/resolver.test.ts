// @vitest-environment node
import { describe, expect, test, vi } from "vitest";
import type { ProbeResult } from "../../../src/main/key/probe";
import { KeyResolver } from "../../../src/main/key/resolver";

const probeOf = (result: ProbeResult | null) => vi.fn(async () => result);

describe("order", () => {
  test("the app's environment wins and the login shell is not asked about the key", async () => {
    const probe = probeOf({ AI_API_KEY: "from-shell" });
    const resolver = new KeyResolver({
      env: { AI_API_KEY: "from-env", PATH: "/opt/bin:/usr/bin" },
      probe,
    });
    expect(await resolver.resolve()).toEqual({ present: true, source: "env" });
    expect(probe).not.toHaveBeenCalled();
    expect(resolver.environment().apiKey).toBe("from-env");
  });

  test("when the shell is asked for its PATH anyway, the environment's key still wins over the shell's", async () => {
    const resolver = new KeyResolver({
      env: { AI_API_KEY: "from-env", PATH: "/usr/bin:/bin:/usr/sbin:/sbin" },
      probe: probeOf({ AI_API_KEY: "from-shell", PATH: "/opt/bin" }),
    });
    expect(await resolver.resolve()).toEqual({ present: true, source: "env" });
    expect(resolver.environment().apiKey).toBe("from-env");
  });

  test("without an environment key the login shell is next", async () => {
    const resolver = new KeyResolver({
      env: { PATH: "/p" },
      probe: probeOf({ AI_API_KEY: "from-shell" }),
    });
    expect(await resolver.resolve()).toEqual({ present: true, source: "login_shell" });
    expect(resolver.environment().variables.AI_API_KEY).toBe("from-shell");
  });

  test("with nothing found the status is none, and an empty probe is not an error", async () => {
    expect(await new KeyResolver({ env: { PATH: "/p" }, probe: probeOf({}) }).resolve()).toEqual({
      present: false,
      source: "none",
    });
    expect(await new KeyResolver({ env: { PATH: "/p" }, probe: probeOf(null) }).resolve()).toEqual({
      present: false,
      source: "none",
    });
    expect(
      await new KeyResolver({
        env: { AI_API_KEY: "   ", PATH: "/p" },
        probe: probeOf(null),
      }).resolve(),
    ).toEqual({ present: false, source: "none" });
  });

  test("a typed key fills the gap, in memory", async () => {
    const resolver = new KeyResolver({ env: { PATH: "/p" }, probe: probeOf(null) });
    await resolver.resolve();
    expect(resolver.setForSession("  sk-typed  ")).toEqual({ present: true, source: "session" });
    expect(resolver.environment()).toMatchObject({ apiKey: "sk-typed" });
    expect(resolver.setForSession("   ")).toEqual({ present: true, source: "session" });
  });

  test("the first resolution is cached, and check again looks at both places once more", async () => {
    const probe = vi
      .fn<() => Promise<ProbeResult | null>>()
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ AI_API_KEY: "now-there" });
    const resolver = new KeyResolver({ env: { PATH: "/p" }, probe });
    await resolver.resolve();
    await resolver.resolve();
    expect(probe).toHaveBeenCalledTimes(1);
    expect(await resolver.resolve(true)).toEqual({ present: true, source: "login_shell" });
    expect(probe).toHaveBeenCalledTimes(2);
  });

  test("check again keeps a typed key when nothing better turns up", async () => {
    const resolver = new KeyResolver({ env: { PATH: "/p" }, probe: probeOf(null) });
    await resolver.resolve();
    resolver.setForSession("typed");
    expect(await resolver.resolve(true)).toEqual({ present: true, source: "session" });
  });
});

describe("the sidecar environment", () => {
  test("a launchd PATH makes the shell worth asking even when the key is in the environment", async () => {
    const probe = probeOf({ PATH: "/opt/homebrew/bin:/usr/bin", https_proxy: "http://proxy:3128" });
    const resolver = new KeyResolver({
      env: {
        AI_API_KEY: "k",
        PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
        HOME: "/h",
        ELECTRON_RUN_AS_NODE: "1",
      },
      probe,
    });
    await resolver.resolve();
    expect(probe).toHaveBeenCalledTimes(1);
    expect(resolver.environment().variables).toEqual({
      HOME: "/h",
      PATH: "/opt/homebrew/bin:/usr/bin",
      https_proxy: "http://proxy:3128",
      AI_API_KEY: "k",
    });
  });

  test("the app's own proxy variables win over the shell's, and nothing else leaks in", async () => {
    const resolver = new KeyResolver({
      env: { PATH: "/p", HTTPS_PROXY: "http://mine", NODE_OPTIONS: "--inspect" },
      probe: probeOf({ AI_API_KEY: "k", https_proxy: "http://theirs", no_proxy: "x" }),
    });
    await resolver.resolve();
    const { variables } = resolver.environment();
    expect(variables.HTTPS_PROXY).toBe("http://mine");
    expect(variables.https_proxy).toBe("http://theirs");
    expect(variables.no_proxy).toBe("x");
    expect(variables.NODE_OPTIONS).toBeUndefined();
  });

  test("no key means no AI_API_KEY in the environment at all", async () => {
    const resolver = new KeyResolver({ env: { PATH: "/p" }, probe: probeOf(null) });
    await resolver.resolve();
    expect(resolver.environment()).toEqual({ variables: { PATH: "/p" }, apiKey: undefined });
  });
});
