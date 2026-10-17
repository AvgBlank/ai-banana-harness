import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, type FakeClient, fakeClient, session } from "./fakes";

const NOW = Date.parse("2026-10-17T12:00:00.000Z");
const SESSIONS = [
  session({
    id: "a1",
    workspace: "/Users/me/alpha",
    title: "Fix flaky test",
    updated_at: "2026-10-17T11:59:30.000Z",
  }),
  session({
    id: "a2",
    workspace: "/Users/me/alpha",
    title: "Add a README",
    updated_at: "2026-10-17T09:00:00.000Z",
  }),
  session({
    id: "b1",
    workspace: "/Users/me/beta",
    title: null,
    updated_at: "2026-10-14T12:00:00.000Z",
  }),
  session({
    id: "c1",
    workspace: "/Users/me/gamma",
    title: "Old work",
    updated_at: "2026-10-01T12:00:00.000Z",
  }),
];

function mockMedia(narrow: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: narrow && query.includes("max-width"),
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
}

async function shell(
  client: FakeClient = fakeClient({ sessions: SESSIONS }),
  theme: "dark" | "light" | "system" = "dark",
) {
  const view = render(
    <ClientProvider client={client}>
      <App prefs={{ ...DEFAULT_PREFS, theme }} />
    </ClientProvider>,
  );
  await screen.findByText("Fix flaky test");
  return { client, ...view };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
  vi.setSystemTime(NOW);
  mockMedia(false);
});
afterEach(() => {
  vi.useRealTimers();
});

describe("the shell", () => {
  test("renders the title bar, rail, sidebar and home with the app name from main", async () => {
    await shell();
    expect(screen.getByRole("banner")).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Primary" })).toBeTruthy();
    expect(screen.getByRole("complementary", { name: "Workspaces and sessions" })).toBeTruthy();
    expect(screen.getByRole("main")).toBeTruthy();
    expect(await screen.findByText("Test App")).toBeTruthy();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("What should we work on?");
    expect(screen.getByLabelText("Task").hasAttribute("disabled")).toBe(true);
  });

  test("every button has a type and every control has a name", async () => {
    const { container } = await shell();
    for (const button of container.querySelectorAll("button")) {
      expect(button.getAttribute("type"), button.outerHTML).toBe("button");
      expect(
        button.getAttribute("aria-label") || button.textContent?.trim(),
        button.outerHTML,
      ).toBeTruthy();
    }
  });
});

describe("the session list", () => {
  test("groups sessions by workspace, newest group first, with relative times", async () => {
    await shell();
    const sidebar = screen.getByRole("complementary", { name: "Workspaces and sessions" });
    const groups = within(sidebar)
      .getAllByRole("button", { expanded: true })
      .map((b) => b.textContent);
    expect(groups).toEqual(["alpha", "beta", "gamma"]);
    expect(within(sidebar).getByText("now")).toBeTruthy();
    expect(within(sidebar).getByText("3h")).toBeTruthy();
    expect(within(sidebar).getByText("3d")).toBeTruthy();
    expect(within(sidebar).getByText("2w")).toBeTruthy();
    expect(within(sidebar).getByText("Untitled session")).toBeTruthy();
  });

  test("a group collapses", async () => {
    await shell();
    fireEvent.click(screen.getByRole("button", { name: "gamma" }));
    expect(screen.queryByText("Old work")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "gamma" }));
    expect(screen.getByText("Old work")).toBeTruthy();
  });

  test("shows a starting note until the sidecar answers, then retries", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    client.listSessions.mockRejectedValueOnce(new Error("sidecar_unavailable"));
    vi.useFakeTimers({
      toFake: ["Date", "setInterval", "clearInterval", "setTimeout", "clearTimeout"],
    });
    vi.setSystemTime(NOW);
    render(
      <ClientProvider client={client}>
        <App prefs={DEFAULT_PREFS} />
      </ClientProvider>,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByText("Starting…")).toBeTruthy();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByText("Fix flaky test")).toBeTruthy();
    expect(screen.queryByText("Starting…")).toBeNull();
  });

  test("says so when there are no sessions", async () => {
    render(
      <ClientProvider client={fakeClient()}>
        <App prefs={DEFAULT_PREFS} />
      </ClientProvider>,
    );
    expect(await screen.findByText(/No sessions yet/)).toBeTruthy();
  });

  test("the running dot comes from the sidecar and a crash turns it into the interrupted marker", async () => {
    const client = fakeClient({ sessions: SESSIONS, running: ["a1"] });
    await shell(client);
    expect(screen.getAllByRole("img", { name: "Turn running" })).toHaveLength(1);
    expect(screen.queryByRole("img", { name: "Interrupted" })).toBeNull();
    act(() => client.pushState({ state: "crashed", attempt: 1, restartInMs: 500 }));
    expect(screen.queryByRole("img", { name: "Turn running" })).toBeNull();
    expect(screen.getAllByRole("img", { name: "Interrupted" })).toHaveLength(1);
  });

  test("a restart that reaches ready reloads the list", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await shell(client);
    const before = client.listSessions.mock.calls.length;
    await act(async () => client.pushState({ state: "ready", reopened: [] }));
    expect(client.listSessions.mock.calls.length).toBe(before + 1);
  });

  test("selecting a session marks it current and puts it in the title bar", async () => {
    await shell();
    fireEvent.click(screen.getByRole("button", { name: /^Add a README/ }));
    const row = screen.getByRole("button", { name: /^Add a README/ });
    expect(row.getAttribute("aria-current")).toBe("true");
    const header = screen.getByRole("banner");
    expect(header.textContent).toContain("Add a README");
    expect(header.textContent).toContain("alpha");
  });

  test("renaming sends the new title through the client", async () => {
    const { client } = await shell();
    fireEvent.doubleClick(screen.getByRole("button", { name: /^Add a README/ }));
    const field = screen.getByLabelText("Rename session");
    expect(document.activeElement).toBe(field);
    fireEvent.change(field, { target: { value: "  A better name  " } });
    fireEvent.blur(field);
    expect(client.renameSession).toHaveBeenCalledWith("/Users/me/alpha", "a2", "A better name");
    expect(screen.queryByLabelText("Rename session")).toBeNull();
  });

  test("an empty or unchanged rename sends nothing", async () => {
    const { client } = await shell();
    fireEvent.doubleClick(screen.getByRole("button", { name: /^Add a README/ }));
    fireEvent.change(screen.getByLabelText("Rename session"), { target: { value: "  " } });
    fireEvent.blur(screen.getByLabelText("Rename session"));
    expect(client.renameSession).not.toHaveBeenCalled();
  });
});

describe("theme", () => {
  test("the saved theme is applied and the settings cards switch and persist it", async () => {
    const { client } = await shell(fakeClient({ sessions: SESSIONS }), "dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "Appearance" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Light" }));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(client.setPrefs).toHaveBeenLastCalledWith({ theme: "light" });
    fireEvent.click(screen.getByRole("button", { name: "System" }));
    expect(document.documentElement.dataset.theme).toBe("system");
    expect(client.setPrefs).toHaveBeenLastCalledWith({ theme: "system" });
    // Back to where it started is a change from what was last saved, so it is saved too.
    fireEvent.click(screen.getByRole("button", { name: "Dark" }));
    expect(client.setPrefs).toHaveBeenLastCalledWith({ theme: "dark" });
    expect(client.setPrefs).toHaveBeenCalledTimes(3);
  });

  test("the pressed card follows the theme, and nothing is saved just for starting", async () => {
    const { client } = await shell(fakeClient({ sessions: SESSIONS }), "light");
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("button", { name: "Light" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Dark" }).getAttribute("aria-pressed")).toBe("false");
    expect(client.setPrefs).not.toHaveBeenCalled();
  });

  test("a failed save leaves the app on the chosen theme", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    client.setPrefs.mockRejectedValue(new Error("disk full"));
    await shell(client);
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "Light" }));
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});

describe("layout rule", () => {
  test("below 1280 px the sidebar steps aside while Changes is open", async () => {
    mockMedia(true);
    await shell();
    const sidebar = () => screen.queryByRole("complementary", { name: "Workspaces and sessions" });
    expect(sidebar()).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Toggle changes panel" }));
    expect(screen.getByRole("complementary", { name: "Changes" })).toBeTruthy();
    expect(sidebar()).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close changes" }));
    expect(sidebar()).not.toBeNull();
  });

  test("at 1280 px and wider both stay", async () => {
    mockMedia(false);
    await shell();
    fireEvent.click(screen.getByRole("button", { name: "Toggle changes panel" }));
    expect(screen.getByRole("complementary", { name: "Changes" })).toBeTruthy();
    expect(screen.getByRole("complementary", { name: "Workspaces and sessions" })).toBeTruthy();
  });

  test("the sidebar toggle hides and shows it", async () => {
    await shell();
    fireEvent.click(screen.getByRole("button", { name: "Toggle sidebar" }));
    expect(screen.queryByRole("complementary", { name: "Workspaces and sessions" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Toggle sidebar" }));
    expect(screen.getByRole("complementary", { name: "Workspaces and sessions" })).toBeTruthy();
  });
});

describe("home", () => {
  test("opening a folder asks main for it and names the workspace", async () => {
    const client = fakeClient({ sessions: SESSIONS, picked: "/Users/me/delta" });
    await shell(client);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    });
    expect(client.openWorkspace).toHaveBeenCalledWith("/Users/me/delta");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "What should we work on in delta?",
    );
    expect(screen.getByRole("banner").textContent).toContain("delta");
  });

  test("a cancelled dialog changes nothing", async () => {
    const client = fakeClient({ sessions: SESSIONS, picked: null });
    await shell(client);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    });
    expect(client.openWorkspace).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("What should we work on?");
  });

  test("a suggestion fills the composer, and the provider picker lists what the sidecar reports", async () => {
    const client = fakeClient({ sessions: SESSIONS, picked: "/Users/me/delta" });
    await shell(client);
    fireEvent.click(screen.getByRole("button", { name: "Explain how a turn is checkpointed" }));
    expect((screen.getByLabelText("Task") as HTMLTextAreaElement).value).toBe(
      "Explain how a turn is checkpointed",
    );

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    });
    const trigger = await screen.findByRole("button", {
      name: "Provider and model: DeepSeek deepseek-flash",
    });
    expect(client.providers).toHaveBeenCalledWith("/Users/me/delta");
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    expect(await screen.findByText("Your key is sent to api.deepseek.com")).toBeTruthy();
    fireEvent.click(screen.getByText("Qwen"));
    expect(
      screen.getByRole("button", { name: "Provider and model: Qwen qwen3.8-flash" }),
    ).toBeTruthy();
  });

  test("the provider picker is off until a workspace is open", async () => {
    await shell();
    const picker = screen.getByRole("button", { name: /Provider and model/ });
    expect(picker.hasAttribute("disabled")).toBe(true);
    expect(waitFor).toBeDefined();
  });
});

describe("words", () => {
  test("the interface never says chat or thread, in any view", async () => {
    const client = fakeClient({ sessions: SESSIONS, picked: "/Users/me/delta" });
    await shell(client);
    const words = /\b(chat|chats|thread|threads)\b/i;
    const text = () => document.body.textContent ?? "";
    expect(text()).not.toMatch(words);
    fireEvent.click(screen.getByRole("button", { name: "Toggle changes panel" }));
    expect(text()).not.toMatch(words);
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(text()).not.toMatch(words);
    fireEvent.click(screen.getByRole("button", { name: "Home" }));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    });
    expect(text()).not.toMatch(words);
  });
});
