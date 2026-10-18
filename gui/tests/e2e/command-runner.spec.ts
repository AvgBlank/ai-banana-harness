import { appendFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Launched, Stub } from "./fixtures";
import { expect, test } from "./fixtures";

const userConfig = (stub: Stub) => join(stub.env.XDG_CONFIG_HOME, "banana-harness", "config.toml");
const workspaceConfig = (stub: Stub) => join(stub.workspace, ".banana-harness", "config.toml");

// Both files are shared by the worker's tests: each starts from, and returns to, its own contents.
const original = new Map<string, string>();
test.beforeEach(({ stub }) => {
  original.set(stub.root, readFileSync(userConfig(stub), "utf8"));
});
test.afterEach(({ stub }) => {
  writeFileSync(userConfig(stub), original.get(stub.root) ?? "");
  rmSync(join(stub.workspace, ".banana-harness"), { recursive: true, force: true });
});

async function pickFolder(app: Launched, stub: Stub): Promise<void> {
  await app.app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);
  const open = app.page.getByRole("button", { name: "Open folder" });
  await expect(open).toBeEnabled({ timeout: 20_000 });
  const name = stub.workspace.split("/").at(-1) as string;
  const settled = app.page
    .getByRole("dialog")
    .or(app.page.getByRole("heading", { level: 1 }).filter({ hasText: name }));
  // The first click can land before the app is listening; click until something answers.
  await expect(async () => {
    await open.click();
    await expect(settled.first()).toBeVisible({ timeout: 2_000 });
  }).toPass({ timeout: 20_000 });
}

const banner = (app: Launched) => app.page.getByRole("banner");

async function openSettings(app: Launched): Promise<void> {
  await app.page.getByRole("button", { name: /Settings/ }).click();
  await expect(app.page.getByRole("heading", { name: "Command execution" })).toBeVisible();
}

test("by default commands run on this Mac and Settings says containers are off", async ({
  launch,
  stub,
}) => {
  const app = await launch();
  await pickFolder(app, stub);
  await expect(banner(app).getByText(/Container/)).toBeHidden();
  await openSettings(app);
  await expect(app.page.getByText(/Containers are off/)).toBeVisible();
});

test("a container the user asked for, with no Docker to reach, is shown as unavailable", async ({
  launch,
  stub,
}) => {
  appendFileSync(
    userConfig(stub),
    `\n[tools.bash]\nrunner = "container"\n[tools.bash.container]\nimage = "dev:1"\n` +
      `docker = "${join(stub.root, "no-such-docker")}"\n`,
  );
  const app = await launch();
  await pickFolder(app, stub);
  await expect(banner(app).getByText("Container unavailable")).toBeVisible();
  await openSettings(app);
  await expect(app.page.getByText("dev:1")).toBeVisible();
  await expect(app.page.getByRole("alert")).toBeVisible();
});

test("a workspace config that sets where commands run is listed as ignored", async ({
  launch,
  stub,
}) => {
  mkdirSync(join(stub.workspace, ".banana-harness"), { recursive: true });
  writeFileSync(
    workspaceConfig(stub),
    `[tools.bash]\nrunner = "container"\n[tools.bash.container]\nimage = "evil:1"\n`,
  );
  const app = await launch();
  await pickFolder(app, stub);
  const dialog = app.page.getByRole("dialog");
  await expect(dialog.getByText("tools.bash.container.image")).toBeVisible();
  await dialog.getByRole("button", { name: "Trust and open" }).click();
  await expect(dialog).toBeHidden();
  await expect(banner(app).getByText(/Container/)).toBeHidden();
  await openSettings(app);
  await expect(app.page.getByText(/Containers are off/)).toBeVisible();
});
