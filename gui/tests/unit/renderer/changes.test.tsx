import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { App } from "@/App";
import { DEBOUNCE_MS } from "@/state/changes";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, fakeClient, fileChange, frame, session } from "./fakes";

const WS = "/w";
let n = 0;
const ev = (type: string, rest: Record<string, unknown> = {}) => ({ id: `e${++n}`, type, ...rest });
const FILES = [
  fileChange({ path: "src/a.rs", added: 3, removed: 1 }),
  fileChange({ path: "src/new.rs", status: "added", added: 5, removed: 0 }),
];

function turnRecord(extra: unknown[] = []) {
  const events = [ev("turn.done", { state: "done" }), ...extra];
  return {
    turn_id: "T1",
    seq: 1,
    input: "edit it",
    state: "done",
    error: null,
    usage: null,
    events,
  };
}

function stubMedia() {
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
}

async function open(
  options: Parameters<typeof fakeClient>[0] = {},
  prefs = { ...DEFAULT_PREFS, changesOpen: true },
) {
  const client = fakeClient({
    sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
    turns: { s1: [turnRecord()] },
    turnFiles: FILES,
    workspaceFiles: [fileChange({ path: "README.md", added: 2, removed: 0 })],
    ...options,
  });
  stubMedia();
  render(
    <ClientProvider client={client}>
      <App prefs={prefs} />
    </ClientProvider>,
  );
  await screen.findByText("Fix it");
  await act(async () => undefined);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Fix it/ })));
  await act(async () => undefined);
  return client;
}

beforeEach(() => {
  n = 0;
});

afterEach(() => {
  vi.useRealTimers();
});

describe("changes panel", () => {
  test("lists the last turn's files with totals", async () => {
    const client = await open();
    const panel = screen.getByRole("complementary", { name: "Changes" });
    expect(client.changesTurn).toHaveBeenCalledWith(WS, "s1");
    const list = within(panel).getByRole("list", { name: "Changed files" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(within(panel).getAllByText("+8").length).toBeGreaterThan(0);
  });

  test("the workspace scope asks for workspace changes", async () => {
    const client = await open();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Scope/ })));
    await act(async () => fireEvent.click(await screen.findByText("Workspace")));
    expect(client.changesWorkspace).toHaveBeenCalledWith(WS);
    expect((await screen.findAllByText("README.md")).length).toBeGreaterThan(0);
  });

  test("a failed request shows Retry, and Retry asks again", async () => {
    const client = await open();
    client.changesTurn.mockRejectedValueOnce(new Error("sidecar busy"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Refresh changes" })));
    const retry = await screen.findByRole("button", { name: "Retry" });
    const before = client.changesTurn.mock.calls.length;
    await act(async () => fireEvent.click(retry));
    expect(client.changesTurn.mock.calls.length).toBe(before + 1);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Retry" })).toBeNull());
  });

  test("mutating tool responses make one refresh after the debounce", async () => {
    const client = await open();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const before = client.changesTurn.mock.calls.length;
    client.pushEvent(frame("s1", "T2", ev("turn.created", { session_id: "s1", turn_id: "T2" })));
    for (const id of ["c1", "c2", "c3"]) {
      const call = { thread_id: "t", tool_call_id: id, name: "edit", arguments: "{}" };
      const done = { thread_id: "t", tool_call_id: id, content: "ok", blocked: false };
      client.pushEvent(frame("s1", "T2", ev("tool.call", call)));
      client.pushEvent(frame("s1", "T2", ev("tool.response", done)));
    }
    await act(async () => vi.advanceTimersByTime(DEBOUNCE_MS - 100));
    expect(client.changesTurn.mock.calls.length).toBe(before);
    await act(async () => vi.advanceTimersByTime(200));
    expect(client.changesTurn.mock.calls.length).toBe(before + 1);
  });
});

describe("refresh triggers", () => {
  test("a read-only tool response does not refresh", async () => {
    const client = await open();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const before = client.changesTurn.mock.calls.length;
    const call = { thread_id: "t", tool_call_id: "r1", name: "read_file", arguments: "{}" };
    const done = { thread_id: "t", tool_call_id: "r1", content: "text", blocked: false };
    client.pushEvent(frame("s1", "T2", ev("tool.call", call)));
    client.pushEvent(frame("s1", "T2", ev("tool.response", done)));
    await act(async () => vi.advanceTimersByTime(DEBOUNCE_MS * 2));
    expect(client.changesTurn.mock.calls.length).toBe(before);
  });

  test("a finished turn refreshes both scopes at once", async () => {
    const client = await open();
    const turn = client.changesTurn.mock.calls.length;
    const workspace = client.changesWorkspace.mock.calls.length;
    await act(async () => client.pushEvent(frame("s1", "T2", ev("turn.done", { state: "done" }))));
    expect(client.changesTurn.mock.calls.length).toBe(turn + 1);
    expect(client.changesWorkspace.mock.calls.length).toBe(workspace + 1);
  });
});

describe("undo", () => {
  const closed = { ...DEFAULT_PREFS, changesOpen: false };

  test("Undo asks first, names the files, and Keep changes does nothing", async () => {
    const client = await open({}, closed);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Undo" })));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("src/a.rs")).toBeTruthy();
    expect(within(dialog).getByText("will be deleted")).toBeTruthy();
    const keep = within(dialog).getByRole("button", { name: "Keep changes" });
    expect(document.activeElement).toBe(keep);
    await act(async () => fireEvent.click(keep));
    expect(client.undo).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("confirming restores, marks the turn undone and leaves a hint", async () => {
    const client = await open({}, closed);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Undo" })));
    await act(async () =>
      fireEvent.click(await screen.findByRole("button", { name: "Undo turn" })),
    );
    expect(client.undo).toHaveBeenCalledWith(WS, "s1");
    expect(await screen.findByText("Undone")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("still remembers the turn");
  });

  test("a failed undo says so and keeps Undo available", async () => {
    const client = await open({}, closed);
    client.undo.mockRejectedValueOnce(new Error("disk full"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Undo" })));
    await act(async () =>
      fireEvent.click(await screen.findByRole("button", { name: "Undo turn" })),
    );
    expect((await screen.findByRole("alert")).textContent).toContain("disk full");
    expect(screen.getByRole("button", { name: "Undo" })).toBeTruthy();
  });

  test("nothing to undo hides Undo without marking the turn", async () => {
    const client = await open({}, closed);
    client.undo.mockResolvedValueOnce({ outcome: "nothing_to_undo" });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Undo" })));
    await act(async () =>
      fireEvent.click(await screen.findByRole("button", { name: "Undo turn" })),
    );
    await waitFor(() => expect(screen.queryByRole("button", { name: "Undo" })).toBeNull());
    expect(screen.queryByText("Undone")).toBeNull();
  });

  test("no checkpoint means no Undo", async () => {
    const client = fakeClient({
      sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
      turns: { s1: [turnRecord()] },
    });
    client.changesTurn.mockResolvedValue({ checkpoint: false, files: [] });
    stubMedia();
    render(
      <ClientProvider client={client}>
        <App prefs={DEFAULT_PREFS} />
      </ClientProvider>,
    );
    await screen.findByText("Fix it");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /^Fix it/ })));
    await act(async () => undefined);
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });

  test("an undone turn replays as undone", async () => {
    await open({ turns: { s1: [turnRecord([ev("turn.undone", { turn_id: "T1" })])] } }, closed);
    expect(await screen.findByText("Undone")).toBeTruthy();
  });
});
