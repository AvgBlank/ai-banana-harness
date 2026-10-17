import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import {
  type CoreEvent,
  optimisticTurn,
  reduceTurn,
  type TurnView,
  turnFromRecord,
} from "@/state/reduceTurn";
import { DEFAULT_PREFS, type FakeClient, fakeClient, frame, session } from "./fakes";

const WS = "/w";
let n = 0;
const ev = (type: string, rest: Record<string, unknown> = {}): CoreEvent => ({
  id: `e${++n}`,
  type,
  ...rest,
});
const call = (id: string, name: string, args: unknown) =>
  ev("tool.call", { thread_id: "t", tool_call_id: id, name, arguments: JSON.stringify(args) });
const approval = (
  action: string,
  callId: string,
  name: string,
  args: unknown,
  rule = 'bash rule "make *" requires approval',
) =>
  ev("tool.approval_required", {
    action_id: action,
    thread_id: "t",
    tool_call_id: callId,
    name,
    arguments: JSON.stringify(args),
    rule,
  });
const question = (action: string, text: string, options: string[] = []) =>
  ev("tool.response_required", { action_id: action, thread_id: "t", question: text, options });
const response = (callId: string, content: string) =>
  ev("tool.response", { thread_id: "t", tool_call_id: callId, content, blocked: false });
const done = (state = "done") => ev("turn.done", { state });
const created = () => ev("turn.created", { session_id: "new1", turn_id: "T1" });

const run = (events: CoreEvent[], from: TurnView = optimisticTurn("go")) =>
  events.reduce(reduceTurn, from);

beforeEach(() => {
  n = 0;
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
});

describe("pending actions in the reducer", () => {
  test("approvals and questions queue oldest first and mark the card awaiting approval", () => {
    const turn = run([
      call("c1", "bash", { command: "make a" }),
      approval("a1", "c1", "bash", { command: "make a" }),
      question("q1", "Which?"),
    ]);
    expect(turn.pending.map((p) => p.actionId)).toEqual(["a1", "q1"]);
    const card = turn.items[0];
    expect(card?.kind === "tool" && card.state).toBe("awaiting_approval");
  });

  test("a response for the call, a turn end, or an interrupted replay removes them", () => {
    const events = [
      call("c1", "bash", { command: "x" }),
      approval("a1", "c1", "bash", {}),
      call("c2", "bash", {}),
      approval("a2", "c2", "bash", {}),
    ];
    expect(run([...events, response("c1", "exit code: 0")]).pending.map((p) => p.actionId)).toEqual(
      ["a2"],
    );
    expect(run([...events, done("cancelled")]).pending).toEqual([]);
    expect(
      turnFromRecord({ turn_id: "T", input: "", state: null, error: null, events }).pending,
    ).toEqual([]);
    const interrupted = turnFromRecord({
      turn_id: "T",
      input: "",
      state: null,
      error: null,
      events,
    });
    expect(interrupted.items.every((i) => i.kind !== "tool" || i.state === "interrupted")).toBe(
      true,
    );
  });

  test("the answer to a question leaves its card and the question leaves the queue", () => {
    const turn = run([
      call("c", "ask_question", { question: "Name?" }),
      question("q", "Name?"),
      response("c", "Ada"),
    ]);
    expect(turn.pending).toEqual([]);
    const card = turn.items[0];
    expect(card?.kind === "tool" && [card.state, card.output]).toEqual(["done", "Ada"]);
  });

  test("the same event twice queues one action", () => {
    const e = approval("a1", "c1", "bash", {});
    expect(run([call("c1", "bash", {}), e, e]).pending).toHaveLength(1);
  });
});

async function startTurn(client: FakeClient) {
  render(
    <ClientProvider client={client}>
      <App prefs={{ ...DEFAULT_PREFS, lastWorkspace: WS }} />
    </ClientProvider>,
  );
  await screen.findByText("Older");
  await act(async () => undefined);
  fireEvent.change(screen.getByLabelText("Task"), { target: { value: "go" } });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
  });
  act(() => client.pushEvent(frame("new1", null, created() as never)));
}

const push = (client: FakeClient, ...events: CoreEvent[]) =>
  act(async () => {
    for (const e of events) client.pushEvent(frame("new1", "T1", e as never, WS));
    await new Promise((r) => setTimeout(r, 40));
  });

const dialog = () => screen.queryByRole("dialog");
const make = (client: FakeClient) => client;

describe("the approval dialog", () => {
  const base = () =>
    fakeClient({ sessions: [session({ id: "s0", workspace: WS, title: "Older" })] });

  test("it shows the tool, command, workspace and rule, with Deny focused", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "make test" }),
      approval("a1", "c1", "bash", { command: "make test" }),
    );
    const box = within(screen.getByRole("dialog"));
    expect(box.getByText("Run this command?")).toBeTruthy();
    expect(box.getByText("make test")).toBeTruthy();
    expect(box.getByText("bash", { selector: "span" })).toBeTruthy();
    expect(box.getByText(WS)).toBeTruthy();
    expect(box.getByText('bash rule "make *" requires approval')).toBeTruthy();
    expect(box.getByText("Why you're asked")).toBeTruthy();
    expect(document.activeElement).toBe(box.getByRole("button", { name: "Deny" }));
    expect(box.queryByText(/ of /)).toBeNull();
  });

  test("several approvals queue oldest first with the count, and each answer goes to the sidecar", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "first" }),
      approval("a1", "c1", "bash", { command: "first" }),
      call("c2", "write_file", { path: "second.txt" }),
      approval("a2", "c2", "write_file", { path: "second.txt" }, 'edit rule "*" requires approval'),
      call("c3", "bash", { command: "third" }),
      approval("a3", "c3", "bash", { command: "third" }),
    );
    expect(within(screen.getByRole("dialog")).getByText("1 of 3")).toBeTruthy();
    expect(within(screen.getByRole("dialog")).getByText("first")).toBeTruthy();

    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Allow once" })));
    expect(client.replyAction).toHaveBeenLastCalledWith(WS, "new1", "a1", "allow");
    expect(within(screen.getByRole("dialog")).getByText("1 of 2")).toBeTruthy();
    expect(within(screen.getByRole("dialog")).getByText("second.txt")).toBeTruthy();
    expect(within(screen.getByRole("dialog")).getByText("Allow this action?")).toBeTruthy();

    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Allow for this session" })),
    );
    expect(client.replyAction).toHaveBeenLastCalledWith(WS, "new1", "a2", "allow_for_session");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Deny" })));
    expect(client.replyAction).toHaveBeenLastCalledWith(WS, "new1", "a3", "deny");
    expect(dialog()).toBeNull();
  });

  test("the dialog closes at once and a refusal from the sidecar changes nothing", async () => {
    const client = make(base());
    client.replyAction.mockRejectedValue(new Error("unknown_action"));
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "x" }),
      approval("a1", "c1", "bash", { command: "x" }),
    );
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Allow once" })));
    expect(dialog()).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("Escape and a click outside do not dismiss it", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "x" }),
      approval("a1", "c1", "bash", { command: "x" }),
    );
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(dialog()).not.toBeNull();
    expect(client.replyAction).not.toHaveBeenCalled();
  });

  test("the tool card reads awaiting approval while the dialog is open", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "x" }),
      approval("a1", "c1", "bash", { command: "x" }),
    );
    expect(screen.getAllByText(/Waiting for your approval/).length).toBeGreaterThan(0);
  });

  test("the end of the turn, and a response for the call, close the dialog", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "x" }),
      approval("a1", "c1", "bash", { command: "x" }),
    );
    expect(dialog()).not.toBeNull();
    await push(client, done("error"));
    expect(dialog()).toBeNull();
  });

  test("Stop closes the dialog and cancels the turn", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "x" }),
      approval("a1", "c1", "bash", { command: "x" }),
    );
    expect(dialog()).not.toBeNull();
    // The dialog is modal for people; the page behind it still answers events, which is all this needs.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Stop", hidden: true }));
    });
    expect(client.cancelTurn).toHaveBeenCalledWith(WS, "new1");
    expect(dialog()).toBeNull();
  });

  test("the sidecar's pending list decides which actions survive a restart", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "bash", { command: "kept" }),
      approval("a1", "c1", "bash", { command: "kept" }),
      call("c2", "bash", { command: "gone" }),
      approval("a2", "c2", "bash", { command: "gone" }),
    );
    expect(within(screen.getByRole("dialog")).getByText("1 of 2")).toBeTruthy();
    client.pendingActionIds.mockResolvedValue(["a1"]);
    await act(async () => client.pushState({ state: "ready", reopened: [] }));
    await act(async () => new Promise((r) => setTimeout(r, 20)));
    expect(dialog()).not.toBeNull();
    expect(within(screen.getByRole("dialog")).queryByText(/ of /)).toBeNull();
    client.pendingActionIds.mockResolvedValue([]);
    await act(async () => client.pushState({ state: "ready", reopened: [] }));
    await act(async () => new Promise((r) => setTimeout(r, 20)));
    expect(dialog()).toBeNull();
  });
});

describe("questions", () => {
  const base = () =>
    fakeClient({ sessions: [session({ id: "s0", workspace: WS, title: "Older" })] });

  test("options and a free-text answer both reply with text", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "ask_question", { question: "Which colour?" }),
      question("q1", "Which colour?", ["red", "blue"]),
    );
    const box = within(screen.getByRole("dialog"));
    expect(box.getByText("Which colour?", { selector: "p.whitespace-pre-wrap" })).toBeTruthy();
    await act(async () => fireEvent.click(box.getByRole("button", { name: "blue" })));
    expect(client.replyAction).toHaveBeenLastCalledWith(WS, "new1", "q1", { text: "blue" });
    expect(dialog()).toBeNull();

    await push(
      client,
      call("c2", "ask_question", { question: "Your name?" }),
      question("q2", "Your name?"),
    );
    const second = within(screen.getByRole("dialog"));
    expect(second.getByRole("button", { name: "Send answer" }).hasAttribute("disabled")).toBe(true);
    fireEvent.change(second.getByLabelText("Your answer"), { target: { value: "  Ada  " } });
    await act(async () => fireEvent.click(second.getByRole("button", { name: "Send answer" })));
    expect(client.replyAction).toHaveBeenLastCalledWith(WS, "new1", "q2", { text: "Ada" });
  });

  test("the card shows the answer from tool.response", async () => {
    const client = make(base());
    await startTurn(client);
    await push(
      client,
      call("c1", "ask_question", { question: "Name?" }),
      question("q1", "Name?"),
      response("c1", "Ada"),
    );
    expect(dialog()).toBeNull();
    expect(screen.getByText("Answer:").textContent).toContain("Ada");
  });
});

describe("the provider picker in a session", () => {
  test("it lists the sidecar's options and switches the open session", async () => {
    const client = fakeClient({ sessions: [session({ id: "s0", workspace: WS, title: "Older" })] });
    render(
      <ClientProvider client={client}>
        <App prefs={{ ...DEFAULT_PREFS, lastWorkspace: WS }} />
      </ClientProvider>,
    );
    await screen.findByText("Older");
    await act(async () => undefined);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older/ })));
    const trigger = await screen.findByRole("button", {
      name: "Provider and model: DeepSeek deepseek-flash",
    });
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    expect(await screen.findByText("Your key is sent to api.deepseek.com")).toBeTruthy();
    expect(screen.getByText("Your key is sent to dashscope.aliyuncs.com")).toBeTruthy();
    expect(screen.getByText(/One model per provider in this version/)).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("Qwen")));
    expect(client.switchProvider).toHaveBeenCalledWith(WS, "s0", "qwen");
    expect(
      await screen.findByRole("button", { name: "Provider and model: Qwen qwen3.8-flash" }),
    ).toBeTruthy();
  });

  test("a refused switch leaves the session's provider alone", async () => {
    const client = fakeClient({ sessions: [session({ id: "s0", workspace: WS, title: "Older" })] });
    client.switchProvider.mockRejectedValue(new Error("turn_in_progress"));
    render(
      <ClientProvider client={client}>
        <App prefs={{ ...DEFAULT_PREFS, lastWorkspace: WS }} />
      </ClientProvider>,
    );
    await screen.findByText("Older");
    await act(async () => undefined);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older/ })));
    const trigger = await screen.findByRole("button", {
      name: "Provider and model: DeepSeek deepseek-flash",
    });
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    await act(async () => fireEvent.click(await screen.findByText("Qwen")));
    expect(
      screen.getByRole("button", { name: "Provider and model: DeepSeek deepseek-flash" }),
    ).toBeTruthy();
  });
});
