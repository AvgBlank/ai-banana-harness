// The packaged app, launched from gui/dist: its window opens, it knows it is packaged, and its
// bundled sidecar says ready. Run through `make gui-package-smoke`.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { _electron as electron, expect, test } from "@playwright/test";

const app = resolve(__dirname, "../../dist/mac-arm64/Banana Harness.app");
const TEST_KEY = "sk-test-0123456789abcdef";

test("the packaged app opens a window and its bundled sidecar is ready", async () => {
  const home = mkdtempSync(join(tmpdir(), "bh-packaged-"));
  const lines: string[] = [];
  const launched = await electron.launch({
    executablePath: join(app, "Contents", "MacOS", "Banana Harness"),
    args: [`--user-data-dir=${join(home, "userdata")}`],
    // A development override must be ignored by a packaged app: the sidecar comes from the bundle.
    env: {
      HOME: home,
      PATH: "/usr/bin:/bin",
      AI_API_KEY: TEST_KEY,
      BANANA_HARNESS_BIN: "/nonexistent/banana-harness",
    },
  });
  try {
    for (const stream of [launched.process().stdout, launched.process().stderr]) {
      stream?.on("data", (chunk: Buffer) => lines.push(chunk.toString("utf8")));
    }
    const packaged = await launched.evaluate(({ app: electronApp }) => electronApp.isPackaged);
    expect(packaged).toBe(true);

    const page = await launched.firstWindow();
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByRole("button", { name: "Open folder" })).toBeVisible();
    expect(
      await launched.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length),
    ).toBe(1);

    // A call only goes through once main has seen the sidecar's ready frame.
    await expect
      .poll(
        () =>
          page.evaluate(() =>
            window.banana.rpc.call("session.list", {}).then(
              () => "ready",
              () => "waiting",
            ),
          ),
        {
          timeout: 30_000,
        },
      )
      .toBe("ready");
    // Main moves to `ready` only on the sidecar's ready frame.
    expect(await page.evaluate(() => window.banana.sidecar.state())).toMatchObject({
      state: "ready",
    });
    expect(lines.join("")).not.toContain(TEST_KEY);
  } finally {
    await launched.close().catch(() => undefined);
  }
});
