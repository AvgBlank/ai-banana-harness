// @vitest-environment node
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import { TrustStore } from "../../../src/main/trust/store";
import { tempDir } from "./tmp";

const HASH = "a".repeat(64);

describe("TrustStore", () => {
  test("a trusted workspace is remembered with its hash and approvals, mode 0600", async () => {
    const dir = tempDir("trust-");
    const file = join(dir, "trust.json");
    const store = new TrustStore(file, () => new Date("2026-10-17T10:00:00Z"));
    await store.trust("/w", { configSha256: HASH, approvedHosts: ["https://x.example"] });
    expect(await new TrustStore(file).get("/w")).toEqual({
      configSha256: HASH,
      approvedHosts: ["https://x.example"],
    });
    expect(statSync(file).mode & 0o777).toBe(0o600);
    expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({
      version: 1,
      workspaces: {
        "/w": {
          config_sha256: HASH,
          approved_hosts: ["https://x.example"],
          trusted_at: "2026-10-17T10:00:00.000Z",
        },
      },
    });
    expect(existsSync(`${file}.tmp`)).toBe(false);
  });

  test("a missing file trusts nothing", async () => {
    const store = new TrustStore(join(tempDir("trust-"), "trust.json"));
    expect(await store.get("/w")).toBeNull();
    expect(await store.list()).toEqual([]);
  });

  test("a corrupt file is moved aside and nothing is trusted", async () => {
    const dir = tempDir("trust-");
    const file = join(dir, "trust.json");
    writeFileSync(file, "{ not json");
    const store = new TrustStore(file);
    expect(await store.get("/w")).toBeNull();
    expect(existsSync(file)).toBe(false);
    expect(readdirSync(dir).some((name) => name.startsWith("trust.json.corrupt-"))).toBe(true);
    await store.trust("/w", { configSha256: HASH, approvedHosts: [] });
    expect(await store.get("/w")).not.toBeNull();
  });

  test("a file of the wrong shape is treated as corrupt", async () => {
    const dir = tempDir("trust-");
    const file = join(dir, "trust.json");
    writeFileSync(
      file,
      JSON.stringify({ version: 1, workspaces: { "/w": { config_sha256: "short" } } }),
    );
    expect(await new TrustStore(file).get("/w")).toBeNull();
    expect(existsSync(file)).toBe(false);
  });

  test("forget removes only that workspace", async () => {
    const store = new TrustStore(join(tempDir("trust-"), "trust.json"));
    await store.trust("/a", { configSha256: HASH, approvedHosts: [] });
    await store.trust("/b", { configSha256: HASH, approvedHosts: [] });
    await store.forget("/a");
    expect((await store.list()).map((w) => w.workspace)).toEqual(["/b"]);
  });

  test("quick changes are all kept", async () => {
    const store = new TrustStore(join(tempDir("trust-"), "trust.json"));
    await Promise.all(
      ["/a", "/b", "/c", "/d"].map((w) =>
        store.trust(w, { configSha256: HASH, approvedHosts: [] }),
      ),
    );
    expect((await store.list()).map((w) => w.workspace)).toEqual(["/a", "/b", "/c", "/d"]);
  });
});
