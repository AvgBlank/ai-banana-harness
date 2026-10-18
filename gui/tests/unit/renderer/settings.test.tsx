import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, fakeClient, session } from "./fakes";

async function openSettings(options: Parameters<typeof fakeClient>[0] = {}) {
  const client = fakeClient(options);
  render(
    <ClientProvider client={client}>
      <App prefs={DEFAULT_PREFS} />
    </ClientProvider>,
  );
  await act(async () => undefined);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /Settings/ })));
  await act(async () => undefined);
  return client;
}

beforeEach(() => {
  window.matchMedia = (() => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
});

describe("settings", () => {
  test("has appearance, the API key, trusted workspaces, shortcuts and about, in that order", async () => {
    await openSettings();
    const headings = within(screen.getByRole("main"))
      .getAllByRole("heading", { level: 2 })
      .map((h) => h.textContent);
    expect(headings).toEqual([
      "Appearance",
      "API key",
      "Command execution",
      "Trusted workspaces",
      "Keyboard shortcuts",
      "About",
    ]);
  });

  test("the key shows its presence and source, never a value", async () => {
    await openSettings({ key: { present: true, source: "login_shell" } });
    expect(await screen.findByText("Key found")).toBeTruthy();
    expect(screen.getByText("login shell")).toBeTruthy();
    expect(screen.getByText("The value is never shown here.")).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/sk-/);
  });

  test("a missing key is said so, and Check again finds one that appeared", async () => {
    const client = await openSettings({
      key: { present: false, source: "none" },
      foundOnRecheck: { present: true, source: "env" },
    });
    expect(await screen.findByText("No key found")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Check again" })));
    expect(client.recheckKey).toHaveBeenCalled();
    expect(await screen.findByText("Key found")).toBeTruthy();
    expect(screen.getByText("environment")).toBeTruthy();
  });

  test("lists the shortcuts that exist and no others", async () => {
    await openSettings();
    const list = screen
      .getByRole("heading", { name: "Keyboard shortcuts" })
      .closest("section") as HTMLElement;
    const rows = within(list)
      .getAllByRole("listitem")
      .map((li) => li.textContent);
    expect(rows).toEqual([
      "Send message↵",
      "New line⇧↵",
      "Close a dialog that can be dismissedEsc",
    ]);
  });

  test("about shows the app version and protocol 1", async () => {
    await openSettings();
    const about = screen.getByRole("heading", { name: "About" }).closest("section") as HTMLElement;
    expect(await within(about).findByText("Test App 0.1.0")).toBeTruthy();
    expect(within(about).getByText("1")).toBeTruthy();
  });

  test("the theme cards switch the theme at once and say which is on", async () => {
    await openSettings();
    fireEvent.click(screen.getByRole("button", { name: /^Light/ }));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(screen.getByRole("button", { name: /^Light/ }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(screen.getByRole("button", { name: /^Dark/ }).getAttribute("aria-pressed")).toBe(
      "false",
    );
  });
});

describe("Escape in dialogs", () => {
  async function withTrustDialog() {
    const client = fakeClient({
      sessions: [session({ id: "s1", workspace: "/w", title: "Fix it" })],
      picked: "/w",
      trust: { "/w": "needs_review" },
    });
    render(
      <ClientProvider client={client}>
        <App prefs={DEFAULT_PREFS} />
      </ClientProvider>,
    );
    await screen.findByText("Fix it");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Open folder" })));
    await screen.findByRole("dialog");
    return client;
  }

  test("Escape closes the trust dialog as Open in restricted mode, and trusts nothing", async () => {
    const client = await withTrustDialog();
    await act(async () => fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(client.trustWorkspace).not.toHaveBeenCalled();
    expect(within(screen.getByRole("banner")).getByText("Restricted")).toBeTruthy();
  });
});
