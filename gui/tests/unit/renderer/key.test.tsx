import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import type { Prefs } from "../../../src/shared/api";
import { DEFAULT_PREFS, type FakeClient, fakeClient, session } from "./fakes";

const SESSIONS = [session({ id: "a1", workspace: "/Users/me/alpha", title: "Past session" })];
const TEST_KEY = "sk-test-0123456789abcdef";
const NO_KEY = { present: false, source: "none" } as const;

async function open(client: FakeClient, prefs: Prefs = DEFAULT_PREFS) {
  const view = render(
    <ClientProvider client={client}>
      <App prefs={prefs} />
    </ClientProvider>,
  );
  await screen.findByText("Past session");
  await act(async () => undefined);
  return view;
}

const openFolderButton = () => screen.getByRole("button", { name: "Open folder" });

function openWorkspaceMenu() {
  const trigger = screen.getByRole("button", { name: /^Workspace:/ });
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  fireEvent.click(trigger);
}

beforeEach(() => {
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
});

describe("without an API key", () => {
  test("the onboarding explains where the key is read from, and past sessions still show", async () => {
    await open(fakeClient({ sessions: SESSIONS, key: NO_KEY }));
    expect(screen.getByRole("heading", { name: "No API key found" })).toBeTruthy();
    expect(screen.getByText(/Test App reads your key only from/)).toBeTruthy();
    expect(screen.getByText("Past session")).toBeTruthy();
  });

  test("opening a workspace and starting a turn are off", async () => {
    await open(
      fakeClient({
        sessions: SESSIONS,
        key: NO_KEY,
        prefs: { ...DEFAULT_PREFS, recents: ["/Users/me/alpha"] },
      }),
    );
    expect(openFolderButton().hasAttribute("disabled")).toBe(true);
    expect(screen.getByLabelText("Task").hasAttribute("disabled")).toBe(true);
    expect((screen.getByLabelText("Task") as HTMLTextAreaElement).placeholder).toBe(
      "Add an API key to start a session.",
    );
    openWorkspaceMenu();
    expect(screen.getByRole("button", { name: /Open folder…/ }).hasAttribute("disabled")).toBe(
      true,
    );
    expect(screen.getByText("/Users/me/alpha").closest("button")?.hasAttribute("disabled")).toBe(
      true,
    );
  });

  test("check again asks main and stays put when nothing is found", async () => {
    const client = fakeClient({ sessions: SESSIONS, key: NO_KEY });
    await open(client);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    });
    expect(client.recheckKey).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("heading", { name: "No API key found" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("check again that finds a key clears the onboarding and allows opening", async () => {
    const client = fakeClient({
      sessions: SESSIONS,
      key: NO_KEY,
      foundOnRecheck: { present: true, source: "login_shell" },
    });
    await open(client);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    });
    expect(screen.queryByRole("heading", { name: "No API key found" })).toBeNull();
    expect(openFolderButton().hasAttribute("disabled")).toBe(false);
  });

  test("a failed check says so", async () => {
    const client = fakeClient({ sessions: SESSIONS, key: NO_KEY });
    client.recheckKey.mockRejectedValue(new Error("x"));
    await open(client);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    });
    expect(screen.getByRole("alert").textContent).toContain("Could not check");
  });

  test("a typed key is handed over once, trimmed, the field is emptied and the key is gone from the page", async () => {
    const client = fakeClient({ sessions: SESSIONS, key: NO_KEY });
    const { container } = await open(client);
    const field = screen.getByLabelText("API key for this session") as HTMLInputElement;
    expect(field.type).toBe("password");
    expect(
      screen.getByRole("button", { name: "Use for this session" }).hasAttribute("disabled"),
    ).toBe(true);
    fireEvent.change(field, { target: { value: `  ${TEST_KEY}  ` } });
    fireEvent.click(screen.getByRole("button", { name: "Show key" }));
    expect(field.type).toBe("text");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Use for this session" }));
    });
    expect(client.setSessionKey).toHaveBeenCalledWith(TEST_KEY);
    expect(client.setSessionKey).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("heading", { name: "No API key found" })).toBeNull();
    expect(container.innerHTML).not.toContain(TEST_KEY);
    expect(document.body.textContent).not.toContain(TEST_KEY);
    expect(openFolderButton().hasAttribute("disabled")).toBe(false);
  });

  test("a rejected key leaves the onboarding up with a message and no key in the field", async () => {
    const client = fakeClient({ sessions: SESSIONS, key: NO_KEY });
    client.setSessionKey.mockRejectedValue(new Error("x"));
    await open(client);
    const field = screen.getByLabelText("API key for this session") as HTMLInputElement;
    fireEvent.change(field, { target: { value: TEST_KEY } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Use for this session" }));
    });
    expect(screen.getByRole("alert").textContent).toContain("could not be used");
    expect(field.value).toBe("");
  });
});

describe("with a key", () => {
  test("there is no onboarding and opening is allowed", async () => {
    await open(fakeClient({ sessions: SESSIONS }));
    expect(screen.queryByRole("heading", { name: "No API key found" })).toBeNull();
    expect(openFolderButton().hasAttribute("disabled")).toBe(false);
  });

  test("the workspace from last time is opened again, restricted, and named", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await open(client, { ...DEFAULT_PREFS, lastWorkspace: "/Users/me/alpha" });
    expect(client.openWorkspace).toHaveBeenCalledWith("/Users/me/alpha");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "What should we work on in alpha?",
    );
  });

  test("a last workspace that can no longer be opened is quietly skipped", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    client.openWorkspace.mockRejectedValue(new Error("workspace_not_found"));
    await open(client, { ...DEFAULT_PREFS, lastWorkspace: "/gone" });
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("What should we work on?");
  });

  test("with no key the last workspace is not opened, and it comes back once a key is typed", async () => {
    const client = fakeClient({ sessions: SESSIONS, key: NO_KEY });
    await open(client, { ...DEFAULT_PREFS, lastWorkspace: "/Users/me/alpha" });
    expect(client.openWorkspace).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("API key for this session"), {
      target: { value: TEST_KEY },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Use for this session" }));
    });
    expect(client.openWorkspace).toHaveBeenCalledWith("/Users/me/alpha");
  });
});

describe("recent workspaces", () => {
  const prefs = { ...DEFAULT_PREFS, recents: ["/Users/me/alpha", "/Users/me/beta"] };

  test("they are listed, and choosing one opens it through the same flow and puts it first", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    await open(client, prefs);
    openWorkspaceMenu();
    expect(screen.getByText("/Users/me/beta")).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByText("/Users/me/beta"));
    });
    expect(client.openWorkspace).toHaveBeenCalledWith("/Users/me/beta");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "What should we work on in beta?",
    );
    openWorkspaceMenu();
    const shown = screen.getAllByText(/^\/Users\/me\//).map((node) => node.textContent);
    // The first is the picker bar naming the current workspace; the rest are the menu rows.
    const paths = [...new Set(shown)];
    expect(paths.slice(0, 2)).toEqual(["/Users/me/beta", "/Users/me/alpha"]);
  });

  test("a recent workspace that fails to open changes nothing", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    client.openWorkspace.mockRejectedValue(new Error("workspace_not_found"));
    await open(client, prefs);
    openWorkspaceMenu();
    await act(async () => {
      fireEvent.click(screen.getByText("/Users/me/beta"));
    });
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("What should we work on?");
  });
});

describe("panel state", () => {
  test("the saved panels are restored and a change is handed to main", async () => {
    const client = fakeClient({ sessions: SESSIONS });
    render(
      <ClientProvider client={client}>
        <App prefs={{ ...DEFAULT_PREFS, sidebarOpen: false, changesOpen: true }} />
      </ClientProvider>,
    );
    await act(async () => undefined);
    expect(screen.queryByRole("complementary", { name: "Workspaces and sessions" })).toBeNull();
    expect(screen.getByRole("complementary", { name: "Changes" })).toBeTruthy();
    expect(client.setPrefs).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Toggle sidebar" }));
    expect(client.setPrefs).toHaveBeenLastCalledWith({ sidebarOpen: true });
    fireEvent.click(screen.getByRole("button", { name: "Close changes" }));
    expect(client.setPrefs).toHaveBeenLastCalledWith({ changesOpen: false });
    expect(client.setPrefs).toHaveBeenCalledTimes(2);
  });
});
