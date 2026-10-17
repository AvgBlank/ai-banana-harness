import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";
import { App } from "@/App";
import { CrashBanner } from "@/components/banners/CrashBanner";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, fakeClient, frame, session } from "./fakes";

const WS = "/w";
const usage = { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 };
let n = 0;
const ev = (type: string, rest: Record<string, unknown> = {}) => ({ id: `e${++n}`, type, ...rest });
const created = ev("turn.created", { session_id: "s1", turn_id: "T1" });
const delta = (text: string) =>
  ev("model.message.delta", { thread_id: "t", message_id: "m1", content: text });
const final = (text: string) =>
  ev("model.message", {
    thread_id: "t",
    message_id: "m1",
    message: { content: text, finish_reason: "stop", usage },
  });
const approval = ev("tool.approval_required", {
  action_id: "a1",
  thread_id: "t",
  tool_call_id: "c1",
  name: "bash",
  arguments: JSON.stringify({ command: "make ship" }),
  rule: 'bash rule "make *" requires approval',
});
const toolCall = ev("tool.call", {
  thread_id: "t",
  tool_call_id: "c1",
  name: "bash",
  arguments: JSON.stringify({ command: "make ship" }),
});
const done = ev("turn.done", { state: "done" });

const record = (state: string | null, events: unknown[]) => ({
  turn_id: "T1",
  seq: 1,
  input: "do it",
  state,
  error: null,
  usage: null,
  events,
});

async function open(options: Parameters<typeof fakeClient>[0]) {
  const client = fakeClient({
    sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
    ...options,
  });
  render(
    <ClientProvider client={client}>
      <App prefs={DEFAULT_PREFS} />
    </ClientProvider>,
  );
  await screen.findByText("Fix it");
  await act(async () => undefined);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Fix it/ })));
  await act(async () => undefined);
  return client;
}

beforeEach(() => {
  // Event ids stay unique across tests: the module-level events above were made once.
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
});

describe("the crash banner", () => {
  const noop = () => undefined;

  test("while restarting it says which attempt of three", () => {
    render(
      <CrashBanner
        state={{ state: "crashed", attempt: 2, restartInMs: 1000 }}
        appName="App"
        onRestart={noop}
      />,
    );
    expect(screen.getByRole("alert").textContent).toContain("Restarting… attempt 2 of 3");
    expect(screen.queryByRole("button", { name: "Restart" })).toBeNull();
  });

  test("when it gives up it offers Restart and the redacted stderr tail on request", () => {
    let restarted = 0;
    render(
      <CrashBanner
        state={{ state: "failed", reason: "restart_budget", message: "panic near [REDACTED]" }}
        appName="App"
        onRestart={() => restarted++}
      />,
    );
    expect(screen.queryByText("panic near [REDACTED]")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show details" }));
    expect(screen.getByText("panic near [REDACTED]")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Restart" }));
    expect(restarted).toBe(1);
  });

  test("it shows nothing while the sidecar is fine", () => {
    const { container } = render(
      <CrashBanner state={{ state: "ready", reopened: [] }} appName="App" onRestart={noop} />,
    );
    expect(container.textContent).toBe("");
  });
});

describe("a crash", () => {
  test("interrupts the running turn, closes its dialog, disables the composer and resumes after ready", async () => {
    const client = await open({
      turns: { s1: [record(null, [created, toolCall, approval])] },
      live: { s1: { frames: [], activeTurnId: "T1" } },
    });
    // Running, with its approval waiting.
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.queryByText("This turn was interrupted")).toBeNull();

    await act(async () => client.pushState({ state: "crashed", attempt: 1, restartInMs: 500 }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("This turn was interrupted")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("Restarting… attempt 1 of 3");
    expect((screen.getByLabelText("Task") as HTMLTextAreaElement).disabled).toBe(true);

    // The sidecar returns: the session is read again, and the turn is interrupted from the database.
    client.attachLive.mockResolvedValue({ frames: [], activeTurnId: null });
    const before = client.resume.mock.calls.length;
    await act(async () => client.pushState({ state: "ready", reopened: [WS] }));
    expect(client.resume.mock.calls.length).toBe(before + 1);
    expect(screen.getByText("This turn was interrupted")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect((screen.getByLabelText("Task") as HTMLTextAreaElement).disabled).toBe(false);
    // Nothing was sent again.
    expect(client.startTurn).not.toHaveBeenCalled();
  });

  test("failed shows Restart, and Restart asks main to restart the sidecar", async () => {
    const client = await open({ turns: { s1: [record("done", [created, final("hi"), done])] } });
    await act(async () =>
      client.pushState({ state: "failed", reason: "restart_budget", message: "tail" }),
    );
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Restart" })));
    expect(client.restartSidecar).toHaveBeenCalled();
  });
});

describe("a reload", () => {
  test("restores the partial message and the pending approval without duplicates", async () => {
    const d1 = delta("Working on ");
    const d2 = delta("it now");
    await open({
      turns: { s1: [record(null, [created, d1, toolCall, approval])] },
      // The buffer repeats what is stored and holds one frame more.
      live: {
        s1: {
          frames: [
            frame("s1", "T1", created),
            frame("s1", "T1", d1),
            frame("s1", "T1", toolCall),
            frame("s1", "T1", approval),
            frame("s1", "T1", d2),
          ],
          activeTurnId: "T1",
        },
      },
    });
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getAllByText("Working on it now")).toHaveLength(1);
    expect(screen.queryByText("This turn was interrupted")).toBeNull();
    // The open dialog hides the rest of the page from the accessibility tree.
    expect(screen.getAllByRole("button", { name: "Stop", hidden: true })).toHaveLength(1);
  });

  test("an attach that lands just after turn.done still ends done", async () => {
    await open({
      turns: { s1: [record(null, [created, delta("Almost")])] },
      live: {
        s1: {
          frames: [
            frame("s1", "T1", created),
            frame("s1", "T1", final("All done.")),
            frame("s1", "T1", done),
          ],
          activeTurnId: null,
        },
      },
    });
    expect(screen.getByText("All done.")).toBeTruthy();
    expect(screen.queryByText("This turn was interrupted")).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
  });

  test("a buffer that lost its early deltas is repaired by the final message", async () => {
    await open({
      turns: { s1: [record(null, [created])] },
      live: {
        s1: {
          frames: [frame("s1", "T1", final("The whole answer.")), frame("s1", "T1", done)],
          activeTurnId: null,
        },
      },
    });
    expect(screen.getByText("The whole answer.")).toBeTruthy();
  });

  test("a turn that is not running is not marked running by an empty buffer", async () => {
    await open({ turns: { s1: [record(null, [created])] } });
    expect(screen.getByText("This turn was interrupted")).toBeTruthy();
  });

  test("at start the renderer attaches to every workspace's running turn", async () => {
    const client = fakeClient({
      sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
      running: ["s1"],
    });
    render(
      <ClientProvider client={client}>
        <App prefs={DEFAULT_PREFS} />
      </ClientProvider>,
    );
    await screen.findByText("Fix it");
    await act(async () => undefined);
    expect(client.attachLive).toHaveBeenCalledWith(WS, "s1");
  });
});
