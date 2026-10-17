import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, type FakeClient, fakeClient, frame, session } from "./fakes";

const WS = "/w";
const SESSIONS = [
  session({ id: "s1", workspace: WS, title: "Older session" }),
  session({ id: "s2", workspace: WS, title: "Crashed session" }),
];
const usage = { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 };

async function openApp(client: FakeClient) {
  render(
    <ClientProvider client={client}>
      <App prefs={{ ...DEFAULT_PREFS, lastWorkspace: WS }} />
    </ClientProvider>,
  );
  await screen.findByText("Older session");
  await act(async () => undefined);
}

const task = () => screen.getByLabelText("Task") as HTMLTextAreaElement;
async function type(text: string) {
  fireEvent.change(task(), { target: { value: text } });
}
async function send() {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
  });
}
const flush = () => act(async () => new Promise((r) => setTimeout(r, 40)));
const push = (f: ReturnType<typeof frame>, client: FakeClient) => client.pushEvent(f);

const created = (turn = "T1") =>
  frame("new1", null, { id: "e1", type: "turn.created", session_id: "new1", turn_id: turn });
const delta = (id: string, content: string) =>
  frame("new1", "T1", {
    id,
    type: "model.message.delta",
    thread_id: "th",
    message_id: "m",
    content,
  });
const done = (state = "done", error?: string) =>
  frame("new1", "T1", { id: "e9", type: "turn.done", state, ...(error ? { error } : {}) });

beforeEach(() => {
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
});

describe("the first turn from home", () => {
  test("the prompt appears at once, the stream renders, and the composer follows the turn", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await type("say hi");
    await send();

    expect(client.createSession).toHaveBeenCalledWith(WS, undefined);
    expect(client.startTurn).toHaveBeenCalledWith(WS, "new1", "say hi");
    expect(screen.getAllByText("say hi").length).toBeGreaterThan(0);
    expect(task().value).toBe("");
    expect(task().placeholder).toBe("A turn is running in this workspace");
    expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();

    act(() => {
      push(created(), client);
      push(
        frame("new1", "T1", { id: "e2", type: "model.message", thread_id: "th", message_id: "m" }),
        client,
      );
      push(delta("e3", "Hello"), client);
      push(delta("e4", " there!"), client);
    });
    await flush();
    expect(screen.getByText("Hello there!")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();

    act(() => {
      push(
        frame("new1", "T1", {
          id: "e5",
          type: "model.message",
          thread_id: "th",
          message_id: "m",
          message: { content: "Hello there!", finish_reason: "stop", usage },
        }),
        client,
      );
      push(done(), client);
    });
    await flush();
    expect(screen.getByText("Hello there!")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
    expect(screen.getByRole("button", { name: "Send" })).toBeTruthy();
    expect(task().placeholder).toMatch(/follow-up/);
  });

  test("the sidebar list is asked again when the turn is created and when it ends", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await type("x");
    await send();
    const before = client.listSessions.mock.calls.length;
    act(() => push(created(), client));
    expect(client.listSessions.mock.calls.length).toBeGreaterThan(before);
    const mid = client.listSessions.mock.calls.length;
    act(() => push(done(), client));
    expect(client.listSessions.mock.calls.length).toBeGreaterThan(mid);
  });

  test("a failed start brings the text back with the error and removes the prompt", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    client.startTurn.mockRejectedValue(new Error("a turn is already running in this workspace"));
    await openApp(client);
    await type("try this");
    await send();
    expect(task().value).toBe("try this");
    expect(screen.getByRole("alert").textContent).toContain("already running");
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
  });

  test("many frames in one burst are applied in a single animation frame", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await type("go");
    await send();
    const raf = vi.spyOn(window, "requestAnimationFrame");
    act(() => {
      push(created(), client);
      for (let n = 0; n < 200; n++) push(delta(`d${n}`, "x"), client);
    });
    expect(raf).toHaveBeenCalledTimes(1);
    await flush();
    expect(screen.getByText("x".repeat(200))).toBeTruthy();
    raf.mockRestore();
  });

  test("Stop sends turn.cancel and reads Stopping until the turn ends", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await type("long");
    await send();
    act(() => push(created(), client));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    });
    expect(client.cancelTurn).toHaveBeenCalledWith(WS, "new1");
    expect(screen.getByRole("button", { name: "Stopping…" }).hasAttribute("disabled")).toBe(true);
    act(() => push(done("cancelled"), client));
    await flush();
    expect(screen.getByText("Stopped.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Send" })).toBeTruthy();
  });

  test("a failed turn shows its error, and a 401 adds the hint to switch provider", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await type("hi");
    await send();
    act(() => {
      push(created(), client);
      push(done("error", "provider returned 401: invalid api key"), client);
    });
    await flush();
    expect(screen.getByText("The turn failed")).toBeTruthy();
    expect(screen.getByText(/invalid api key/)).toBeTruthy();
    expect(screen.getByText(/Switch provider with the picker/)).toBeTruthy();
  });

  test("another error gets no provider hint", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await type("hi");
    await send();
    act(() => {
      push(created(), client);
      push(done("error", "the model produced too much"), client);
    });
    await flush();
    expect(screen.queryByText(/Switch provider/)).toBeNull();
  });
});

describe("a failed start in an open session", () => {
  test("the prompt leaves the transcript, the composer is free and the text and error are back", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    client.startTurn.mockRejectedValue(new Error("boom"));
    await openApp(client);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older session/ })));
    await type("doomed prompt");
    await send();
    expect(screen.queryByText("doomed prompt", { selector: "div" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
    expect(screen.getByRole("button", { name: "Send" })).toBeTruthy();
    expect(task().value).toBe("doomed prompt");
    expect(screen.getByRole("alert").textContent).toContain("boom");
  });
});

describe("opening and renaming sessions", () => {
  const record = (id: string, state: string | null, text?: string) => ({
    turn_id: `T-${id}`,
    seq: 1,
    input: `asked in ${id}`,
    state,
    error: null,
    usage: null,
    events: text
      ? [
          {
            id: `${id}a`,
            type: "model.message",
            thread_id: "th",
            message_id: "m",
            message: { content: text, finish_reason: "stop", usage },
          },
          { id: `${id}b`, type: "turn.done", state: "done" },
        ]
      : [],
  });

  test("choosing a session opens its workspace restricted, resumes it and renders its turns", async () => {
    const client = fakeClient({
      sessions: SESSIONS,
      turns: { s1: [record("s1", "done", "an old answer")] },
    });
    await openApp(client);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /^Older session/ }));
    });
    expect(client.openWorkspace).toHaveBeenCalledWith(WS);
    expect(client.resume).toHaveBeenCalledWith(WS, "s1");
    expect(screen.getByText("asked in s1")).toBeTruthy();
    expect(screen.getByText("an old answer")).toBeTruthy();
  });

  test("switching sessions shows the other transcript, and an unfinished turn reads interrupted", async () => {
    const client = fakeClient({
      sessions: SESSIONS,
      turns: { s1: [record("s1", "done", "first answer")], s2: [record("s2", null)] },
    });
    await openApp(client);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older session/ })));
    expect(screen.getByText("first answer")).toBeTruthy();
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: /^Crashed session/ })),
    );
    expect(screen.queryByText("first answer")).toBeNull();
    expect(screen.getByText("This turn was interrupted")).toBeTruthy();
  });

  test("a session whose workspace cannot be opened says why", async () => {
    const client = fakeClient({ sessions: SESSIONS, key: { present: false, source: "none" } });
    client.openWorkspace.mockRejectedValue(new Error("api_key_missing"));
    render(
      <ClientProvider client={client}>
        <App prefs={DEFAULT_PREFS} />
      </ClientProvider>,
    );
    await screen.findByText("Older session");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older session/ })));
    expect(screen.getByRole("alert").textContent).toContain("Add an API key to open this session");
  });

  test("the sent turn on a resumed session goes to that session", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older session/ })));
    await type("and another thing");
    await send();
    expect(client.startTurn).toHaveBeenCalledWith(WS, "s1", "and another thing");
    expect(client.createSession).not.toHaveBeenCalled();
  });

  test("renaming is optimistic and goes back when the sidecar refuses", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    let refuse: (e: Error) => void = () => undefined;
    client.renameSession.mockImplementation(
      () =>
        new Promise((_, reject) => {
          refuse = reject;
        }),
    );
    await openApp(client);
    fireEvent.doubleClick(screen.getByRole("button", { name: /^Older session/ }));
    const field = screen.getByLabelText("Rename session");
    fireEvent.change(field, { target: { value: "Better name" } });
    fireEvent.blur(field);
    expect(screen.getByText("Better name")).toBeTruthy();
    await act(async () => refuse(new Error("nope")));
    await waitFor(() => expect(screen.getByText("Older session")).toBeTruthy());
    expect(screen.queryByText("Better name")).toBeNull();
  });
});

describe("scrolling", () => {
  function scrolled(el: HTMLElement, top: number) {
    Object.defineProperty(el, "scrollHeight", { configurable: true, value: 1000 });
    Object.defineProperty(el, "clientHeight", { configurable: true, value: 400 });
    el.scrollTop = top;
    fireEvent.scroll(el);
  }

  test("scrolling up shows Jump to latest, jumping hides it, and sending reattaches", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await openApp(client);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Older session/ })));
    const scroller = document.querySelector(".overflow-y-auto.h-full") as HTMLElement;
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
    scrolled(scroller, 100);
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Jump to latest" }));
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
    scrolled(scroller, 100);
    await type("again");
    await send();
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
  });
});
