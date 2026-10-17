import { existsSync, readFileSync } from "node:fs";
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

test("bash-and-test: the command card shows its exit chip and survives a reload and resume", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies(scenarioReplies("bash-and-test"));
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "run the check");
  await app.page.getByRole("button", { name: "Allow once" }).click();

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

interface Scenario {
  replies: unknown[];
  expect: {
    requests?: number;
    request_contains?: Array<{ index: number; text: string }>;
    files?: Record<string, string>;
  };
}

function scenario(name: string): Scenario {
  const file = join(__dirname, "../../../crates/harness-testkit/scenarios", `${name}.json`);
  return JSON.parse(readFileSync(file, "utf8")) as Scenario;
}

/** The model-facing and file expectations every seam shares. */
async function expectScenario(stub: Stub, name: string, denied: string[] = []): Promise<void> {
  const { expect: want } = scenario(name);
  const seen = await requests(stub);
  if (want.requests !== undefined) expect(seen).toHaveLength(want.requests);
  for (const { index, text } of want.request_contains ?? []) {
    expect(JSON.stringify(seen[index]?.body.messages)).toContain(text);
  }
  for (const [path, content] of Object.entries(want.files ?? {})) {
    expect(readFileSync(join(stub.workspace, path), "utf8")).toBe(content);
  }
  for (const path of denied) expect(existsSync(join(stub.workspace, path))).toBe(false);
  expect(seen.every((r) => r.auth_ok)).toBe(true);
}

test("edit-and-approve: allow once for the edit, deny for the command", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies(scenario("edit-and-approve").replies);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "write a note and deploy it");

  const dialog = app.page.getByRole("dialog");
  await expect(dialog.getByText("Allow this action?")).toBeVisible();
  await expect(dialog.getByText("NOTES.md")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Deny" })).toBeFocused();
  await dialog.getByRole("button", { name: "Allow once" }).click();

  await expect(dialog.getByText("Run this command?")).toBeVisible();
  await expect(dialog.getByText("touch deployed.txt")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Deny" })).toBeFocused();
  await dialog.getByRole("button", { name: "Deny" }).click();

  await expect(app.page.getByText("Done. I wrote NOTES.md.")).toBeVisible();
  await expect(dialog).toBeHidden();
  await expectScenario(stub, "edit-and-approve", ["deployed.txt"]);
});

test("edit-and-approve: allow for this session skips the second prompt for the same rule", async ({
  launch,
  stub,
  queueReplies,
}) => {
  const write = (id: string, path: string) => ({
    sse: { tool_calls: [{ id, name: "write_file", arguments: { path, content: `${path}\n` } }] },
  });
  await queueReplies([
    write("c1", "one.txt"),
    write("c2", "two.txt"),
    { sse: { text: "Both written." } },
  ]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "write two files");

  await app.page
    .getByRole("dialog")
    .getByRole("button", { name: "Allow for this session" })
    .click();
  await expect(app.page.getByText("Both written.")).toBeVisible();
  await expect(app.page.getByRole("dialog")).toBeHidden();
  expect(readFileSync(join(stub.workspace, "two.txt"), "utf8")).toBe("two.txt\n");
  expect(await requests(stub)).toHaveLength(3);
});

test("question-and-answer: the typed answer goes back to the model and shows on the card", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies(scenario("question-and-answer").replies);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "greet me");

  const dialog = app.page.getByRole("dialog");
  await expect(dialog.getByText("What is your name?", { exact: true }).first()).toBeVisible();
  await dialog.getByLabel("Your answer").fill("Ada");
  await dialog.getByRole("button", { name: "Send answer" }).click();

  await expect(app.page.getByText("Nice to meet you, Ada.")).toBeVisible();
  await expect(app.page.getByText("Answer:")).toContainText("Ada");
  await expectScenario(stub, "question-and-answer");
});

test("a turn that is stopped while it waits closes its dialog", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies(scenario("edit-and-approve").replies);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "write a note");
  await expect(app.page.getByRole("dialog")).toBeVisible();
  // Another way out of a waiting turn: the sidecar is told to cancel it.
  await app.page.evaluate(async () => {
    const [{ workspace }] = (
      (await window.banana.rpc.call("workspace.list", {})) as {
        workspaces: Array<{ workspace: string }>;
      }
    ).workspaces;
    const sessions = (
      (await window.banana.rpc.call("session.list", {})) as {
        sessions: Array<{ id: string; workspace: string }>;
      }
    ).sessions;
    const mine = sessions.find((s) => s.workspace === workspace);
    await window.banana.rpc.call("turn.cancel", {
      workspace,
      session_id: (mine as { id: string }).id,
    });
  });
  await expect(app.page.getByRole("dialog")).toBeHidden();
  await expect(app.page.getByRole("button", { name: "Send" })).toBeVisible();
});

test("edit-and-undo: the panel shows the edit, Undo restores the file and marks the turn", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies([
    {
      sse: {
        tool_calls: [
          { id: "c1", name: "write_file", arguments: { path: "made.txt", content: "hi\n" } },
        ],
      },
    },
    {
      sse: {
        tool_calls: [{ id: "c2", name: "bash", arguments: { command: "touch by-bash.txt" } }],
      },
    },
    { sse: { text: "Wrote made.txt." } },
  ]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "make a file");
  const approve = app.page.getByRole("dialog").getByRole("button", { name: "Allow once" });
  await approve.click();
  await expect(app.page.getByRole("dialog").getByText("touch by-bash.txt")).toBeVisible();
  await approve.click();
  await expect(app.page.getByText("Wrote made.txt.")).toBeVisible();
  expect(existsSync(join(stub.workspace, "made.txt"))).toBe(true);

  await app.page.getByRole("button", { name: "View changes" }).click();
  const panel = app.page.getByRole("complementary", { name: "Changes" });
  await expect(
    panel.getByRole("list", { name: "Changed files" }).getByText("made.txt"),
  ).toBeVisible();

  await app.page.getByRole("button", { name: "Undo", exact: true }).click();
  const dialog = app.page.getByRole("dialog");
  await expect(dialog.getByText("made.txt")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Keep changes" })).toBeFocused();
  await dialog.getByRole("button", { name: "Undo turn" }).click();

  await expect(app.page.getByText("Undone", { exact: true })).toBeVisible();
  await expect(app.page.getByRole("status")).toContainText("still remembers the turn");
  expect(existsSync(join(stub.workspace, "made.txt"))).toBe(false);
  expect(existsSync(join(stub.workspace, "by-bash.txt"))).toBe(false);
  await expect(app.page.getByRole("button", { name: "Undo", exact: true })).toBeHidden();

  // After a reload the replayed event still marks the turn undone, with no Undo offered.
  await app.page.reload();
  await app.page
    .getByRole("complementary", { name: "Workspaces and sessions" })
    .getByRole("button", { name: /make a file/ })
    .first()
    .click();
  await expect(app.page.getByText("Undone", { exact: true })).toBeVisible();
  await expect(app.page.getByRole("button", { name: "Undo", exact: true })).toBeHidden();
});
