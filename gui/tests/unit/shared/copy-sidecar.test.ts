// @vitest-environment node
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import { tempDir } from "../main/tmp";

const script = join(__dirname, "../../../scripts/copy-sidecar.mjs");
const run = (...args: string[]) =>
  spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });

describe("copy-sidecar refuses what must not be packaged", () => {
  test("no path", () => {
    const result = run();
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("usage");
  });

  test("a missing binary", () => {
    const result = run(join(tempDir("sidecar-"), "target", "release", "banana-harness"));
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("does not exist");
  });

  test("a debug build", () => {
    const dir = join(tempDir("sidecar-"), "target", "debug");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "banana-harness"), "#!/bin/sh\n", { mode: 0o755 });
    const result = run(join(dir, "banana-harness"));
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("debug build");
  });

  test("a folder", () => {
    const dir = join(tempDir("sidecar-"), "target", "release");
    mkdirSync(dir, { recursive: true });
    const result = run(dir);
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("not a file");
  });
});
