import { readFileSync } from "node:fs";
import { join } from "node:path";
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

/** The scenario's own replies, so the UI tests and every other seam run the same script. */
function scenarioReplies(name: string): unknown[] {
  const file = join(__dirname, "../../../crates/harness-testkit/scenarios", `${name}.json`);
  return (JSON.parse(readFileSync(file, "utf8")) as { replies: unknown[] }).replies;
}

/**
 * Answers every approval request with Allow through the real protocol. The approval dialog is a
 * later feature; until it exists, the test plays the user's part so the turn can run.
 */
async function allowEverything(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.banana.on("sidecarEvent", (payload) => {
      const frame = payload as {
        workspace: string;
        session_id: string;
        event: { type: string; action_id?: string };
      };
      if (frame.event.type !== "tool.approval_required") return;
      void window.banana.rpc.call("action.reply", {
        workspace: frame.workspace,
        session_id: frame.session_id,
        action_id: frame.event.action_id as string,
        answer: "allow",
      });
    });
  });
}

test("bash-and-test: the command card shows its exit chip and survives a reload and resume", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies(scenarioReplies("bash-and-test"));
  const app = await launch();
  await openFolder(app, stub);
  await allowEverything(app.page);
  await sendPrompt(app.page, "run the check");

  await expect(app.page.getByText("The check printed ok.")).toBeVisible();
  const card = app.page.getByRole("button", { name: /bash/ });
  await expect(card).toContainText("sh check.sh");
  await expect(card).toContainText("exit 0");
  await card.click();
  await expect(app.page.getByText("ok", { exact: true })).toBeVisible();

  // The stub saw the tool result go back to the model.
  const seen = await requests(stub);
  expect(seen).toHaveLength(2);
  expect(JSON.stringify(seen[1]?.body.messages)).toContain("ok");

  // The same card from the stored events.
  await app.page.reload();
  const sidebar = app.page.getByRole("complementary", { name: "Workspaces and sessions" });
  await sidebar
    .getByRole("button", { name: /run the check/ })
    .first()
    .click();
  const replayed = app.page.getByRole("button", { name: /bash/ });
  await expect(replayed).toContainText("exit 0");
  await expect(app.page.getByText("The check printed ok.")).toBeVisible();
});

test("subagents: two children nest in their own cards and nothing leaks to the root", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies(scenarioReplies("subagents"));
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "split the work");

  await expect(app.page.getByText("Both subagents finished.")).toBeVisible();
  const alpha = app.page.getByRole("region", { name: "Subagent alpha" });
  const beta = app.page.getByRole("region", { name: "Subagent beta" });
  await expect(alpha.getByText("alpha done")).toBeVisible();
  await expect(beta.getByText("beta done")).toBeVisible();
  await expect(app.page.getByText("alpha done")).toHaveCount(1);
  await expect(app.page.getByText("done", { exact: true })).toHaveCount(2);

  await app.page.reload();
  const sidebar = app.page.getByRole("complementary", { name: "Workspaces and sessions" });
  await sidebar
    .getByRole("button", { name: /split the work/ })
    .first()
    .click();
  await expect(
    app.page.getByRole("region", { name: "Subagent alpha" }).getByText("alpha done"),
  ).toBeVisible();
  await expect(
    app.page.getByRole("region", { name: "Subagent beta" }).getByText("beta done"),
  ).toBeVisible();
});
