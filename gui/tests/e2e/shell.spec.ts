import { expect, test } from "./fixtures";

test("the sidebar lists the sidecar's real sessions, grouped by workspace, and shows the app name from main", async ({
  launch,
  stub,
}) => {
  const { app, page } = await launch();
  await app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);
  await expect(page.getByRole("navigation", { name: "Primary" })).toBeVisible();
  const sidebar = page.getByRole("complementary", { name: "Workspaces and sessions" });
  await expect(sidebar).toBeVisible();

  // Make a session through the real sidecar, then see it listed after a reload.
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const picked = (await window.banana.workspace.pick()) as string;
        const opened = await window.banana.workspace.open(picked).catch(() => null);
        if (!opened) return false;
        await window.banana.rpc.call("session.create", { workspace: opened.workspace });
        return true;
      }),
    )
    .toBe(true);
  await page.reload();
  const folder = stub.workspace.split("/").at(-1) as string;
  await expect(sidebar.getByRole("button", { name: folder })).toBeVisible();
  await expect(sidebar.getByText("Untitled session").first()).toBeVisible();
  await expect(sidebar.getByText("now").first()).toBeVisible();

  const name = await page.evaluate(() => window.banana.app.info().then((info) => info.name));
  expect(name).toBe("Banana Harness");
  await expect(sidebar).toContainText(name);
  // Main remembers the workspace it opened, so the reload comes back to it.
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    `What should we work on in ${folder}?`,
  );
});

test("a theme chosen in settings is kept by main and applied on the next launch", async ({
  launch,
}) => {
  const first = await launch();
  expect(await first.page.evaluate(() => document.documentElement.dataset.theme)).toBe("dark");
  await first.page.getByRole("button", { name: "Settings" }).click();
  await first.page.getByRole("button", { name: "Light" }).click();
  await expect
    .poll(() => first.page.evaluate(() => document.documentElement.dataset.theme))
    .toBe("light");
  // Persisted by main, not by the page: ask main directly.
  await expect
    .poll(() => first.page.evaluate(() => window.banana.prefs.get()))
    .toMatchObject({ theme: "light" });
  await first.app.close();

  const second = await launch();
  expect(await second.page.evaluate(() => document.documentElement.dataset.theme)).toBe("light");
});
