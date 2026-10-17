import type { Page } from "@playwright/test";
import type { Launched, Stub } from "./fixtures";
import { expect, test } from "./fixtures";

/** Opens the stub's workspace the way a user does: Open folder, with main's dialog answered. */
async function openFolder(app: Launched, stub: Stub): Promise<void> {
  await app.app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);
  const open = app.page.getByRole("button", { name: "Open folder" });
  await expect(open).toBeEnabled({ timeout: 20_000 });
  // The sidecar answers once it is ready; a failed attempt leaves the page as it was, so retry.
  const name = stub.workspace.split("/").at(-1) as string;
  await expect(async () => {
    await open.click();
    await expect(app.page.getByRole("heading", { level: 1 })).toContainText(name, {
      timeout: 2_000,
    });
  }).toPass({ timeout: 20_000 });
}

async function sendPrompt(page: Page, text: string): Promise<void> {
  await page.getByLabel("Task").fill(text);
  await page.getByRole("button", { name: "Send" }).click();
}

async function requests(
  stub: Stub,
): Promise<
  Array<{ auth_ok: boolean; body: { messages: Array<{ role: string; content: string }> } }>
> {
  return (await fetch(`${stub.stubs.baseline.control_url}/requests`)).json();
}

test("plain-answer: the prompt goes to the model and the streamed answer is on screen", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("Hello there!");
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "say hi");

  await expect(app.page.getByText("Hello there!")).toBeVisible();
  await expect(app.page.getByText("say hi").first()).toBeVisible();
  await expect(app.page.getByRole("button", { name: "Send" })).toBeVisible();

  // The same model-facing checks as the in-process seams: one request, the key accepted, the prompt in it.
  const seen = await requests(stub);
  expect(seen).toHaveLength(1);
  expect(seen[0]?.auth_ok).toBe(true);
  expect(JSON.stringify(seen[0]?.body.messages)).toContain("say hi");

  // The sidebar lists the new session under its workspace with the auto-title.
  const sidebar = app.page.getByRole("complementary", { name: "Workspaces and sessions" });
  // The database is shared by the worker, so earlier tests' sessions are listed too: newest first.
  await expect(sidebar.getByText("say hi").first()).toBeVisible();
});

test("a session is resumed from the sidecar after a reload", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("Hello there!");
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "say hi");
  await expect(app.page.getByText("Hello there!")).toBeVisible();

  await app.page.reload();
  const sidebar = app.page.getByRole("complementary", { name: "Workspaces and sessions" });
  await sidebar
    .getByRole("button", { name: /say hi/ })
    .first()
    .click();
  await expect(app.page.getByText("Hello there!")).toBeVisible();
  await expect(app.page.getByText("say hi").first()).toBeVisible();
});

test("provider-error-401: the turn shows the error and the hint to switch provider", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies([{ status: 401, error: '{"error":{"message":"invalid api key"}}' }]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "say hi");

  await expect(app.page.getByText("The turn failed")).toBeVisible();
  await expect(app.page.getByText(/Switch provider with the picker/)).toBeVisible();
  // The composer is free again.
  await expect(app.page.getByRole("button", { name: "Send" })).toBeVisible();
});

test("a rename is kept by the sidecar", async ({ launch, stub, queueAnswer }) => {
  await queueAnswer("Hello there!");
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "say hi");
  await expect(app.page.getByText("Hello there!")).toBeVisible();
  const sidebar = app.page.getByRole("complementary", { name: "Workspaces and sessions" });
  await sidebar
    .getByRole("button", { name: /say hi/ })
    .first()
    .dblclick();
  const field = sidebar.getByLabel("Rename session");
  await field.fill("A better name");
  await field.press("Enter");
  await expect(sidebar.getByText("A better name")).toBeVisible();
  await app.page.reload();
  await expect(sidebar.getByText("A better name")).toBeVisible();
});
