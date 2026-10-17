import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { _electron, test as base, type ElectronApplication, type Page } from "@playwright/test";

const here = __dirname;
const gui = resolve(here, "../..");
const scenarioFile = resolve(gui, "../crates/harness-testkit/scenarios/plain-answer.json");

/** The `fake-provider serve` description line. */
export interface Stub {
  root: string;
  workspace: string;
  env: Record<string, string>;
  stubs: { baseline: { provider_url: string; control_url: string } };
}

export interface Launched {
  app: ElectronApplication;
  page: Page;
  /** Everything the main process wrote to stdout and stderr so far. */
  mainLog: () => string;
}

export interface LaunchOptions {
  /** Run this executable instead of the real sidecar (main honours it only when unpackaged). */
  sidecar?: string;
}

function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set; run these tests through 'make gui-e2e'`);
  return value;
}

interface Fixtures {
  launch: (options?: LaunchOptions) => Promise<Launched>;
  /** Queues a plain text answer on the stub for the next turn. */
  queueAnswer: (text: string) => Promise<void>;
  /** A wrapper script that runs the real sidecar through `support/noisy-sidecar.mjs`. */
  noisySidecar: () => string;
}

interface WorkerFixtures {
  stub: Stub;
}

export const test = base.extend<Fixtures, WorkerFixtures>({
  // One fake provider per worker: its description line gives the isolated environment.
  stub: [
    // biome-ignore lint/correctness/noEmptyPattern: Playwright requires a destructured first argument
    async ({}, use) => {
      const child: ChildProcessWithoutNullStreams = spawn(
        required("FAKE_PROVIDER_BIN"),
        ["serve", "--scenario", scenarioFile],
        { stdio: "pipe" },
      );
      const stub = await new Promise<Stub>((resolveLine, reject) => {
        let buffer = "";
        child.stdout.on("data", (chunk: Buffer) => {
          buffer += chunk.toString("utf8");
          const newline = buffer.indexOf("\n");
          if (newline >= 0) resolveLine(JSON.parse(buffer.slice(0, newline)) as Stub);
        });
        child.on("exit", () => reject(new Error("fake-provider exited before describing itself")));
      });
      await use(stub);
      // Closing stdin is how the fake provider is told to stop and remove its root.
      child.stdin.end();
      await new Promise<void>((done) => child.on("exit", () => done()));
    },
    { scope: "worker" },
  ],

  queueAnswer: async ({ stub }, use) => {
    await use(async (text) => {
      const control = stub.stubs.baseline.control_url;
      await fetch(`${control}/reset`, { method: "POST" });
      const response = await fetch(`${control}/replies`, {
        method: "POST",
        body: JSON.stringify([{ sse: { text } }]),
      });
      if (!response.ok) throw new Error(`queueing a reply failed: ${response.status}`);
    });
  },

  noisySidecar: async ({ stub }, use) => {
    await use(() => {
      const wrapper = join(stub.root, "noisy-sidecar.sh");
      const script = join(here, "support/noisy-sidecar.mjs");
      writeFileSync(
        wrapper,
        `#!/bin/sh\nexec "${process.execPath}" "${script}" "${required("BANANA_HARNESS_BIN")}" "$@"\n`,
      );
      chmodSync(wrapper, 0o755);
      return wrapper;
    });
  },

  launch: async ({ stub }, use, testInfo) => {
    const running: ElectronApplication[] = [];
    let log = "";
    await use(async (options = {}) => {
      const userData = join(stub.root, `userdata-${testInfo.testId}`);
      mkdirSync(userData, { recursive: true });
      // The app directory, not the bundle: the name and title come from its package.json.
      const args = [gui, `--user-data-dir=${userData}`];
      // Chromium's sandbox cannot start in some Linux CI containers; nowhere else.
      if (process.env.CI && process.platform === "linux") args.push("--no-sandbox");
      // `env` replaces the process environment: the isolated one from the fake provider, nothing else.
      const app = await _electron.launch({
        args,
        env: { ...stub.env, BANANA_HARNESS_BIN: options.sidecar ?? required("BANANA_HARNESS_BIN") },
      });
      running.push(app);
      const child = app.process();
      for (const stream of [child.stdout, child.stderr]) {
        stream?.on("data", (chunk: Buffer) => {
          log += chunk.toString("utf8");
        });
      }
      const page = await app.firstWindow();
      await page.waitForLoadState("domcontentloaded");
      return { app, page, mainLog: () => log };
    });
    // Failure artifacts under gui/test-results/<test>/: the main process log and a screenshot.
    if (testInfo.status !== testInfo.expectedStatus) {
      await testInfo.attach("main-process.log", { body: log, contentType: "text/plain" });
      for (const app of running) {
        const page = app.windows()[0];
        if (page)
          await testInfo.attach("window.png", {
            body: await page.screenshot(),
            contentType: "image/png",
          });
      }
    }
    for (const app of running) await app.close().catch(() => undefined);
  },
});

export { expect } from "@playwright/test";
