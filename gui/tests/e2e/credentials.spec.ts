import { execSync } from "node:child_process";
import { chmodSync, existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { PROBE_BEGIN, PROBE_END } from "../../src/main/key/probe";
import type { Launched, Stub } from "./fixtures";
import { expect, test } from "./fixtures";

const TEST_KEY = "sk-test-0123456789abcdef";

/** A stand-in for `$SHELL`: whatever arguments it gets, it runs `body` (a /bin/sh script). */
function stubShell(stub: Stub, name: string, body: string): string {
  const file = join(stub.root, name);
  writeFileSync(file, `#!/bin/sh\n${body}\n`);
  chmodSync(file, 0o755);
  return file;
}

const block = (lines: string[]) =>
  `printf '%s\\n' ${PROBE_BEGIN}; ${lines.map((l) => `echo ${l}`).join("; ")}; printf '%s\\n' ${PROBE_END}`;
const noKeyEnv = (shell: string) => ({ AI_API_KEY: undefined, SHELL: shell });

/** Every file under `dir` that contains `needle`. */
function filesContaining(dir: string, needle: string): string[] {
  const found: string[] = [];
  const walk = (path: string) => {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const full = join(path, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile() && statSync(full).size < 64 * 1024 * 1024) {
        if (readFileSync(full).includes(needle)) found.push(full);
      }
    }
  };
  walk(dir);
  return found;
}

async function sidecarArgv(): Promise<string> {
  return execSync("ps -eo args", { encoding: "utf8" })
    .split("\n")
    .filter((line) => line.includes("serve --stdio"))
    .join("\n");
}

/** Opens the stub's workspace through the real flow: main's own dialog, then a restricted open. */
async function openWorkspace(launched: Launched, stub: Stub): Promise<string> {
  await launched.app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);
  // Until the sidecar says ready, main refuses calls; wait for the first open that goes through.
  await expect
    .poll(
      () =>
        launched.page.evaluate(async () => {
          const picked = (await window.banana.workspace.pick()) as string;
          return window.banana.workspace.open(picked).then(
            (result) => result.workspace,
            (error: { code: string }) => error.code,
          );
        }),
      { timeout: 20_000 },
    )
    .toBe(stub.workspace);
  return stub.workspace;
}

async function runTurn(page: Page, workspace: string): Promise<string[]> {
  return page.evaluate(async (ws) => {
    const types: string[] = [];
    window.banana.on("sidecarEvent", (payload) =>
      types.push((payload as { event: { type: string } }).event.type),
    );
    const created = (await window.banana.rpc.call("session.create", { workspace: ws })) as {
      session: { id: string };
    };
    await window.banana.rpc.call("turn.start", {
      workspace: ws,
      session_id: created.session.id,
      input: "hi",
    });
    const deadline = Date.now() + 20_000;
    while (!types.includes("turn.done") && Date.now() < deadline)
      await new Promise((r) => setTimeout(r, 50));
    return types;
  }, workspace);
}

async function providerSawTheKey(stub: Stub): Promise<boolean[]> {
  const response = await fetch(`${stub.stubs.baseline.control_url}/requests`);
  return ((await response.json()) as Array<{ auth_ok: boolean }>).map((r) => r.auth_ok);
}

async function assertKeyNowhere(launched: Launched, seenByRenderer: unknown[]) {
  expect(filesContaining(launched.userData, TEST_KEY), "the key is on disk in userData").toEqual(
    [],
  );
  expect(launched.mainLog()).not.toContain(TEST_KEY);
  expect(JSON.stringify(seenByRenderer)).not.toContain(TEST_KEY);
  expect(await sidecarArgv()).not.toContain(TEST_KEY);
  expect(await launched.page.content()).not.toContain(TEST_KEY);
}

test("a key in the app's environment is used and appears nowhere it should not", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("ok");
  const app = await launch();
  const status = await app.page.evaluate(() => window.banana.key.status());
  expect(status).toEqual({ present: true, source: "env" });
  const workspace = await openWorkspace(app, stub);
  const types = await runTurn(app.page, workspace);
  expect(types.at(-1)).toBe("turn.done");
  expect(await providerSawTheKey(stub)).toEqual([true]);
  const seen = await app.page.evaluate(async () => [
    await window.banana.key.status(),
    await window.banana.app.info(),
    await window.banana.prefs.get(),
  ]);
  await assertKeyNowhere(app, seen);
});

test("the login shell's key is found, noise and all, and the sidecar gets it", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("ok");
  const shell = stubShell(
    stub,
    "good-shell",
    `echo "Last login: today"; echo AI_API_KEY=decoy-before; ${block([`AI_API_KEY=${TEST_KEY}`, "PATH=/usr/bin:/bin"])}; echo "bye"`,
  );
  const app = await launch({ env: noKeyEnv(shell) });
  expect(await app.page.evaluate(() => window.banana.key.status())).toEqual({
    present: true,
    source: "login_shell",
  });
  const workspace = await openWorkspace(app, stub);
  expect((await runTurn(app.page, workspace)).at(-1)).toBe("turn.done");
  expect(await providerSawTheKey(stub)).toEqual([true]);
  await assertKeyNowhere(app, [await app.page.evaluate(() => window.banana.key.status())]);
});

test("a shell that hangs is skipped after five seconds and the app still opens", async ({
  launch,
  stub,
}) => {
  const shell = stubShell(stub, "hanging-shell", "sleep 60");
  const started = Date.now();
  const app = await launch({ env: noKeyEnv(shell) });
  await expect(app.page.getByRole("heading", { name: "No API key found" })).toBeVisible({
    timeout: 20_000,
  });
  const took = Date.now() - started;
  expect(took).toBeGreaterThan(4_000);
  expect(took).toBeLessThan(15_000);
  expect(await app.page.evaluate(() => window.banana.key.status())).toEqual({
    present: false,
    source: "none",
  });
});

test("with no key anywhere the app opens in needs_key, then a typed key restarts the sidecar and works", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("typed key works");
  const shell = stubShell(stub, "empty-shell", block(["HOME=/nowhere"]));
  const app = await launch({ env: noKeyEnv(shell) });
  const { page } = app;

  expect(await page.evaluate(() => window.banana.key.status())).toEqual({
    present: false,
    source: "none",
  });
  await expect(page.getByRole("heading", { name: "No API key found" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open folder" })).toBeDisabled();
  await expect(page.getByLabel("Task")).toBeDisabled();
  // The sidecar answers without a key (past sessions work); opening a workspace does not.
  await expect(
    page.evaluate(() => window.banana.rpc.call("session.list", {})),
  ).resolves.toBeTruthy();
  await app.app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);
  const refused = await page.evaluate(async () => {
    const picked = (await window.banana.workspace.pick()) as string;
    return window.banana.workspace.open(picked).then(
      () => "opened",
      (e: { code: string }) => e.code,
    );
  });
  expect(refused).toBe("api_key_missing");

  await page.getByLabel("API key for this session").fill(TEST_KEY);
  await page.getByRole("button", { name: "Use for this session" }).click();
  await expect(page.getByRole("heading", { name: "No API key found" })).toBeHidden();
  expect(await page.evaluate(() => window.banana.key.status())).toEqual({
    present: true,
    source: "session",
  });
  await expect(page.getByLabel("API key for this session")).toHaveCount(0);

  // The sidecar was restarted with the key: opening now works and the provider sees it.
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          const picked = (await window.banana.workspace.pick()) as string;
          return window.banana.workspace.open(picked).then(
            (o) => o.workspace,
            (e: { code: string }) => e.code,
          );
        }),
      { timeout: 20_000 },
    )
    .toBe(stub.workspace);
  expect((await runTurn(page, stub.workspace)).at(-1)).toBe("turn.done");
  expect(await providerSawTheKey(stub)).toEqual([true]);
  await assertKeyNowhere(app, [await page.evaluate(() => window.banana.key.status())]);
  // Not remembered: a new launch without any key is back to needs_key.
  await app.app.close();
  const again = await launch({ env: noKeyEnv(shell) });
  expect(await again.page.evaluate(() => window.banana.key.status())).toEqual({
    present: false,
    source: "none",
  });
});

test("check again finds a key that appears in the login shell later", async ({ launch, stub }) => {
  const marker = join(stub.root, "key-is-ready");
  const shell = stubShell(
    stub,
    "late-shell",
    `if [ -f ${marker} ]; then ${block([`AI_API_KEY=${TEST_KEY}`])}; else ${block(["HOME=/nowhere"])}; fi`,
  );
  const { page } = await launch({ env: noKeyEnv(shell) });
  await expect(page.getByRole("heading", { name: "No API key found" })).toBeVisible();

  await page.getByRole("button", { name: "Check again" }).click();
  await expect(page.getByRole("button", { name: "Check again" })).toBeEnabled();
  await expect(page.getByRole("heading", { name: "No API key found" })).toBeVisible();

  writeFileSync(marker, "");
  await page.getByRole("button", { name: "Check again" }).click();
  await expect(page.getByRole("heading", { name: "No API key found" })).toBeHidden();
  expect(await page.evaluate(() => window.banana.key.status())).toEqual({
    present: true,
    source: "login_shell",
  });
});

test("preferences keep the last workspace, the recents and the panels, and hold no secrets", async ({
  launch,
  stub,
}) => {
  const first = await launch();
  const workspace = await openWorkspace(first, stub);
  await first.page.getByRole("button", { name: "Toggle changes panel" }).click();
  await first.page.getByRole("button", { name: "Toggle sidebar" }).click();
  await expect
    .poll(() =>
      first.page.evaluate(() =>
        window.banana.prefs.get().then((p) => [p.changesOpen, p.sidebarOpen]),
      ),
    )
    .toEqual([true, false]);

  const file = join(first.userData, "preferences.json");
  expect(existsSync(file)).toBe(true);
  expect(statSync(file).mode & 0o777).toBe(0o600);
  const stored = JSON.parse(readFileSync(file, "utf8"));
  expect(stored).toMatchObject({
    lastWorkspace: workspace,
    recents: [workspace],
    changesOpen: true,
    sidebarOpen: false,
  });
  expect(Object.keys(stored).sort()).toEqual([
    "changesOpen",
    "changesScope",
    "changesWidth",
    "lastWorkspace",
    "profile",
    "recents",
    "sidebarOpen",
    "theme",
  ]);
  await first.app.close();

  const second = await launch();
  await expect(second.page.getByRole("heading", { level: 1 })).toHaveText(
    `What should we work on in ${workspace.split("/").at(-1)}?`,
  );
  await expect(second.page.getByRole("complementary", { name: "Changes" })).toBeVisible();
  await expect(
    second.page.getByRole("complementary", { name: "Workspaces and sessions" }),
  ).toHaveCount(0);
  await second.page.getByRole("button", { name: /^Workspace:/ }).click();
  await expect(second.page.getByText(workspace).first()).toBeVisible();
});

test("sign out forgets a typed key and restarts the sidecar; signing in again works", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("after signing in again");
  const shell = stubShell(stub, "empty-shell-signout", block(["HOME=/nowhere"]));
  const app = await launch({ env: noKeyEnv(shell) });
  const { page } = app;
  const seen: unknown[] = [];

  await page.getByLabel("API key for this session").fill(TEST_KEY);
  await page.getByRole("button", { name: "Use for this session" }).click();
  await expect(page.getByLabel("API key for this session")).toHaveCount(0);
  await openWorkspace(app, stub);

  await page.getByRole("button", { name: /Settings/ }).click();
  await expect(page.getByText("Signed in", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Sign out" }).click();

  // Back to the no-key state with the onboarding panel, and the key is nowhere.
  await expect(page.getByRole("heading", { name: "No API key found" })).toBeVisible();
  seen.push(await page.evaluate(() => window.banana.key.status()));
  expect(seen[0]).toEqual({ present: false, source: "none" });
  const refused = await page.evaluate(async () => {
    const picked = (await window.banana.workspace.pick()) as string;
    return window.banana.workspace.open(picked).then(
      () => "opened",
      (e: { code: string }) => e.code,
    );
  });
  expect(refused).toBe("api_key_missing");
  await assertKeyNowhere(app, seen);

  // Signing in again brings the workspace back and the provider sees the key.
  await page.getByLabel("API key for this session").fill(TEST_KEY);
  await page.getByRole("button", { name: "Use for this session" }).click();
  await expect(page.getByLabel("API key for this session")).toHaveCount(0);
  const workspace = await openWorkspace(app, stub);
  expect(await runTurn(page, workspace)).toContain("turn.done");
  expect(await providerSawTheKey(stub)).toEqual([true]);
  await assertKeyNowhere(app, seen);
});

test("a key from the environment cannot be signed out of, and the sign-out channel refuses it", async ({
  launch,
}) => {
  const app = await launch();
  const { page } = app;
  await page.getByRole("button", { name: /Settings/ }).click();
  await expect(page.getByText(/cannot be removed here/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign out" })).toBeDisabled();
  const refusal = await page.evaluate(() =>
    window.banana.key.signOut().then(
      () => "signed out",
      (e: { code: string }) => e.code,
    ),
  );
  expect(refusal).toBe("key_not_removable");
  expect(await page.evaluate(() => window.banana.key.status())).toMatchObject({ present: true });
});
