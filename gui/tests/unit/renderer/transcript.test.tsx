import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, fakeClient, session } from "./fakes";

const WS = "/w";
const usage = { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 };
let n = 0;
const ev = (type: string, rest: Record<string, unknown> = {}) => ({ id: `e${++n}`, type, ...rest });
const call = (thread: string, id: string, name: string, args: unknown) =>
  ev("tool.call", {
    thread_id: thread,
    tool_call_id: id,
    name,
    arguments: typeof args === "string" ? args : JSON.stringify(args),
  });
const response = (
  thread: string,
  id: string,
  content: string,
  extra: Record<string, unknown> = {},
) =>
  ev("tool.response", { thread_id: thread, tool_call_id: id, content, blocked: false, ...extra });
const text = (thread: string, message: string, content: string) =>
  ev("model.message", {
    thread_id: thread,
    message_id: message,
    message: { content, finish_reason: "stop", usage },
  });

const DIFF = "--- before\n+++ after\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n";

async function openWith(events: unknown[], state: string | null = "done") {
  const client = fakeClient({
    sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
    turns: {
      s1: [
        { turn_id: "T1", seq: 1, input: "do the thing", state, error: null, usage: null, events },
      ],
    },
  });
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
  render(
    <ClientProvider client={client}>
      <App prefs={DEFAULT_PREFS} />
    </ClientProvider>,
  );
  await screen.findByText("Fix it");
  await act(async () => undefined);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Fix it/ })));
  return client;
}

beforeEach(() => {
  n = 0;
});

describe("tool cards", () => {
  test("a bash card shows the command and its exit chip, and opens to the output", async () => {
    await openWith([
      call("t", "c", "bash", { command: "make test" }),
      response("t", "c", "exit code: 2\nerror: boom"),
      text("t", "m", "It failed."),
      ev("turn.done", { state: "done" }),
    ]);
    const card = screen.getByRole("button", { name: /bash/ });
    expect(card.textContent).toContain("make test");
    expect(card.textContent).toContain("exit 2");
    expect(screen.queryByText("error: boom")).toBeNull();
    fireEvent.click(card);
    expect(screen.getByText("error: boom")).toBeTruthy();
    expect(screen.getByText("It failed.")).toBeTruthy();
  });

  test("a running card shows the spinner and a finished one the check", async () => {
    await openWith(
      [
        call("t", "a", "grep", { pattern: "x" }),
        call("t", "b", "glob", { pattern: "*.rs" }),
        response("t", "b", "src/a.rs"),
      ],
      null,
    );
    const cards = screen.getAllByRole("button", { name: /grep|glob/ });
    expect(
      within(cards[0] as HTMLElement).queryByRole("img", { name: "Interrupted" }),
    ).toBeTruthy();
    expect(within(cards[1] as HTMLElement).queryByRole("img", { name: "Done" })).toBeTruthy();
  });

  test("a blocked call says which rule stopped it", async () => {
    await openWith([
      call("t", "c", "bash", { command: "git push" }),
      response("t", "c", 'Blocked by guardrail: bash rule "git push*"', { blocked: true }),
    ]);
    expect(screen.getByText(/Blocked by guardrail/).textContent).toContain('bash rule "git push*"');
    expect(screen.getByText(/The command did not run/)).toBeTruthy();
  });

  test("invalid argument text keeps the card and shows the raw text", async () => {
    await openWith([call("t", "c", "bash", "{not json")]);
    const card = screen.getByRole("button", { name: /bash/ });
    expect(card.textContent).toContain("{not json");
  });

  test("an edit card shows +N −M and an inline diff", async () => {
    await openWith([
      call("t", "c", "edit", { path: "src/a.rs" }),
      response("t", "c", "Edited src/a.rs", { diff: DIFF }),
    ]);
    const card = screen.getByRole("button", { name: /edit/ });
    expect(card.textContent).toContain("src/a.rs");
    expect(card.textContent).toContain("+1");
    expect(card.textContent).toContain("−1");
    fireEvent.click(card);
    const diff = screen.getByLabelText("Diff");
    expect(within(diff).getByText("new")).toBeTruthy();
    expect(within(diff).getByText("old")).toBeTruthy();
    expect(diff.textContent).not.toContain("--- before");
  });

  test("long output collapses to 200 lines until Show all", async () => {
    const output = `exit code: 0\n${Array.from({ length: 300 }, (_, i) => `row ${i}`).join("\n")}`;
    await openWith([call("t", "c", "bash", { command: "seq" }), response("t", "c", output)]);
    fireEvent.click(screen.getByRole("button", { name: /bash/ }));
    expect(screen.getByText(/row 199/)).toBeTruthy();
    expect(screen.queryByText(/row 250/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show all" }));
    expect(screen.getByText(/row 299/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show all" })).toBeNull();
  });

  test("short output has no Show all", async () => {
    await openWith([
      call("t", "c", "bash", { command: "ls" }),
      response("t", "c", "exit code: 0\na\nb"),
    ]);
    fireEvent.click(screen.getByRole("button", { name: /bash/ }));
    expect(screen.queryByRole("button", { name: "Show all" })).toBeNull();
  });
});

describe("subagent groups", () => {
  const events = () => [
    call("root", "pa", "create_sub_agent", { name: "alpha", input: "x" }),
    call("root", "pb", "create_sub_agent", { name: "beta", input: "y" }),
    ev("thread.created", { thread_id: "ta", parent: "root", title: "alpha" }),
    ev("thread.created", { thread_id: "tb", parent: "root", title: "beta" }),
    call("ta", "c1", "bash", { command: "echo alpha" }),
    text("tb", "m", "beta finished"),
    ev("thread.done", { thread_id: "tb" }),
    ev("thread.done", { thread_id: "ta", error: "alpha crashed" }),
    response("root", "pa", "failed"),
    response("root", "pb", "ok"),
    text("root", "m9", "Both subagents finished."),
    ev("turn.done", { state: "done" }),
  ];

  test("each group holds its own child's work and none of it shows at the root", async () => {
    await openWith(events());
    const alpha = screen.getByRole("region", { name: "Subagent alpha" });
    const beta = screen.getByRole("region", { name: "Subagent beta" });
    expect(within(alpha).getByRole("button", { name: /bash/ })).toBeTruthy();
    expect(within(beta).getByText("beta finished")).toBeTruthy();
    expect(screen.getAllByText("beta finished")).toHaveLength(1);
    expect(screen.getByText("Both subagents finished.")).toBeTruthy();
  });

  test("the group status rolls up and a failed child shows its error", async () => {
    await openWith(events());
    expect(screen.getByText("failed")).toBeTruthy();
    expect(screen.getByText("done")).toBeTruthy();
    expect(screen.getByText("alpha crashed")).toBeTruthy();
  });

  test("a group collapses", async () => {
    await openWith(events());
    const header = screen.getByRole("button", { name: /^alpha/ });
    fireEvent.click(header);
    expect(screen.queryByRole("region", { name: "Subagent alpha" })).toBeNull();
  });
});

describe("notices, interruption and the summary card", () => {
  test("compaction shows a notice", async () => {
    await openWith([
      ev("context.compacted", { thread_id: "t", usage }),
      text("t", "m", "carrying on"),
      ev("turn.done", { state: "done" }),
    ]);
    expect(screen.getByText("Context compacted · 10 tokens")).toBeTruthy();
  });

  test("an unfinished stored turn reads interrupted with its running tool marked", async () => {
    await openWith([call("t", "c", "bash", { command: "sleep 99" })], null);
    expect(screen.getByText("This turn was interrupted")).toBeTruthy();
    expect(screen.getByRole("img", { name: "Interrupted" })).toBeTruthy();
  });

  test("the summary derives files and line counts, and View changes opens the panel", async () => {
    await openWith([
      call("t", "1", "edit", { path: "a.rs" }),
      response("t", "1", "ok", { diff: DIFF }),
      call("t", "2", "write_file", { path: "b.rs" }),
      response("t", "2", "ok", { diff: "--- before\n+++ after\n@@ -0,0 +1,2 @@\n+1\n+2\n" }),
      ev("turn.done", { state: "done" }),
    ]);
    expect(screen.getByText(/Edited 2 files/).textContent).toContain("+3");
    expect(screen.getByText(/Edited 2 files/).textContent).toContain("−1");
    expect(screen.queryByRole("complementary", { name: "Changes" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "View changes" }));
    expect(screen.getByRole("complementary", { name: "Changes" })).toBeTruthy();
  });

  test("a turn that edited nothing has no summary card", async () => {
    await openWith([text("t", "m", "just words"), ev("turn.done", { state: "done" })]);
    expect(screen.queryByText(/Edited/)).toBeNull();
  });

  test("the words chat and thread never appear in the transcript", async () => {
    await openWith([
      call("root", "p", "create_sub_agent", { name: "alpha" }),
      ev("thread.created", { thread_id: "ta", parent: "root", title: "alpha" }),
      text("ta", "m", "child"),
      ev("thread.done", { thread_id: "ta" }),
      response("root", "p", "ok"),
      ev("turn.done", { state: "done" }),
    ]);
    expect(document.body.textContent).not.toMatch(/\b(chat|chats|thread|threads)\b/i);
  });
});
