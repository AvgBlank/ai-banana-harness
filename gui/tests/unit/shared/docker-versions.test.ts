// @vitest-environment node
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import { tempDir } from "../main/tmp";

const repo = join(__dirname, "../../../..");
const script = join(repo, "docker/versions.sh");
const run = (root: string) => spawnSync("sh", [script, root], { encoding: "utf8" });

function fakeRoot(over: { node?: string; engines?: string; rust?: string; pnpm?: string } = {}) {
  const root = tempDir("versions-");
  mkdirSync(join(root, "gui"));
  writeFileSync(
    join(root, "rust-toolchain.toml"),
    `[toolchain]\nchannel = "${over.rust ?? "1.95"}"\nprofile = "minimal"\n`,
  );
  writeFileSync(join(root, "gui/.node-version"), `${over.node ?? "24"}\n`);
  writeFileSync(
    join(root, "gui/package.json"),
    `{\n  "packageManager": "pnpm@${over.pnpm ?? "10.29.2"}",\n  "engines": {\n    "node": "${over.engines ?? "^24.15.0"}"\n  }\n}\n`,
  );
  return root;
}

describe("docker/versions.sh", () => {
  test("agrees with the files that own the versions in this repository", () => {
    const result = run(repo);
    expect(result.status).toBe(0);
    const rust = /channel = "([^"]+)"/.exec(
      readFileSync(join(repo, "rust-toolchain.toml"), "utf8"),
    );
    const pkg = readFileSync(join(repo, "gui/package.json"), "utf8");
    expect(result.stdout).toContain(`RUST_VERSION=${rust?.[1]}`);
    expect(result.stdout).toContain(`PNPM_VERSION=${/pnpm@([\d.]+)/.exec(pkg)?.[1]}`);
    expect(result.stdout).toMatch(/NODE_VERSION=24\.\d+\.\d+/);
  });

  test("prints the three versions it was given", () => {
    const result = run(fakeRoot({ rust: "1.99", pnpm: "11.0.1" }));
    expect(result.stdout.trim().split("\n")).toEqual([
      "RUST_VERSION=1.99",
      "NODE_VERSION=24.22.0",
      "PNPM_VERSION=11.0.1",
    ]);
  });

  test("fails when Node's major differs from gui/.node-version", () => {
    const result = run(fakeRoot({ node: "25" }));
    expect(result.status).toBe(1);
    expect(result.stderr).toContain(".node-version");
  });

  test("fails when the pinned Node is below the engines floor", () => {
    const result = run(fakeRoot({ engines: "^24.30.0" }));
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("engines floor");
  });

  test("fails when a source file has no version", () => {
    expect(run(fakeRoot({ rust: "" })).status).toBe(1);
    expect(run(fakeRoot({ pnpm: "" })).status).toBe(1);
  });
});
