import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import type { Prefs } from "../../../src/shared/api";
import { DEFAULT_PREFS, fakeClient, session } from "./fakes";

async function openSettings(
  options: Parameters<typeof fakeClient>[0] = {},
  prefs: Prefs = DEFAULT_PREFS,
) {
  const client = fakeClient(options);
  render(
    <ClientProvider client={client}>
      <App prefs={prefs} />
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
  test("has the profile first, then appearance, the API key, trusted workspaces, shortcuts and about, in that order", async () => {
    await openSettings();
    const headings = within(screen.getByRole("main"))
      .getAllByRole("heading", { level: 2 })
      .map((h) => h.textContent);
    expect(headings).toEqual([
      "Profile",
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

describe("profile", () => {
  const field = () => screen.getByLabelText("Display name") as HTMLInputElement;
  const type = async (value: string, key: "blur" | "Enter" = "blur") => {
    await act(async () => fireEvent.change(field(), { target: { value } }));
    if (key === "Enter") {
      await act(async () => field().focus());
      await act(async () => fireEvent.keyDown(field(), { key: "Enter" }));
    } else await act(async () => fireEvent.blur(field()));
  };

  test("starts empty with a placeholder avatar and no sidebar avatar", async () => {
    await openSettings();
    expect(field().value).toBe("");
    expect(screen.queryByRole("button", { name: "Clear" })).toBeNull();
    expect(screen.queryByRole("img", { name: /^Profile:/ })).toBeNull();
  });

  test("shows the saved name with its initial, in Settings and the sidebar", async () => {
    await openSettings({}, { ...DEFAULT_PREFS, profile: { displayName: "ada" } });
    expect(field().value).toBe("ada");
    expect(screen.getByRole("img", { name: "Profile: ada" }).textContent).toBe("A");
  });

  test("saves on blur and on Enter, trimmed", async () => {
    const client = await openSettings();
    await type("  Ada  ");
    expect(client.setDisplayName).toHaveBeenLastCalledWith("Ada");
    expect(await screen.findByRole("img", { name: "Profile: Ada" })).toBeTruthy();
    await type("Grace", "Enter");
    expect(client.setDisplayName).toHaveBeenLastCalledWith("Grace");
  });

  test("a too-long name shows an error, is not sent, and the old name stays saved", async () => {
    const client = await openSettings({}, { ...DEFAULT_PREFS, profile: { displayName: "Ada" } });
    await type("x".repeat(61));
    expect((await screen.findByRole("alert")).textContent).toMatch(/60/);
    expect(client.setDisplayName).not.toHaveBeenCalled();
    expect(screen.getByRole("img", { name: "Profile: Ada" })).toBeTruthy();
  });

  test("a refusal from main is shown", async () => {
    const client = await openSettings();
    client.setDisplayName.mockRejectedValueOnce({ code: "invalid_arguments", message: "refused" });
    await type("Ada");
    expect((await screen.findByRole("alert")).textContent).toContain("refused");
  });

  test("Clear, and an empty name, go back to no profile", async () => {
    const client = await openSettings({}, { ...DEFAULT_PREFS, profile: { displayName: "Ada" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Clear" })));
    expect(client.setDisplayName).toHaveBeenLastCalledWith("");
    expect(field().value).toBe("");
    expect(screen.queryByRole("img", { name: /^Profile:/ })).toBeNull();
  });

  test("the profile section names the sign-in state but never shows a key value", async () => {
    const synthetic = "sk-test-synthetic-0123456789";
    const client = await openSettings({ key: { present: true, source: "session" } });
    // A status from main is `{ present, source }`; even a stray value would have no field here.
    expect(Object.keys(await client.keyStatus()).sort()).toEqual(["present", "source"]);
    const section = screen
      .getByRole("heading", { name: "Profile" })
      .closest("section") as HTMLElement;
    expect(section.textContent).toContain("Signed in");
    expect(section.textContent).not.toContain(synthetic);
    expect(document.body.innerHTML).not.toContain(synthetic);
  });
});

describe("sign in and sign out", () => {
  const typed = { present: true, source: "session" as const };

  test("a key from the environment says so, and Sign out is disabled", async () => {
    await openSettings();
    expect(screen.getByText("Signed in")).toBeTruthy();
    expect(screen.getByText(/cannot be removed here/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Sign out" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  test("with no key it says not signed in, and Sign in goes to the onboarding panel", async () => {
    await openSettings({ key: { present: false, source: "none" } });
    expect(screen.getByText("Not signed in")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Sign out" })).toBeNull();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Sign in" })));
    expect(screen.getByLabelText("API key for this session")).toBeTruthy();
  });

  test("signing out a typed key asks main, then shows the no-key state with the panel", async () => {
    const client = await openSettings({ key: typed });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Sign out" })));
    expect(client.signOut).toHaveBeenCalledWith();
    expect(await screen.findByLabelText("API key for this session")).toBeTruthy();
    expect(screen.queryByText("Signed in")).toBeNull();
  });

  test("a refusal from main is shown, and the signed-in state stays", async () => {
    const client = await openSettings({ key: typed });
    client.signOut.mockRejectedValueOnce({
      code: "turn_running",
      message: "stop the running turn before signing out",
    });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Sign out" })));
    expect((await screen.findByRole("alert")).textContent).toContain("stop the running turn");
    expect(screen.getByText("Signed in")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Sign out" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
  });

  test("the display name is untouched by signing out, and no call carries it", async () => {
    const client = await openSettings(
      { key: typed },
      { ...DEFAULT_PREFS, profile: { displayName: "Ada" } },
    );
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Sign out" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /Settings/ })));
    expect((screen.getByLabelText("Display name") as HTMLInputElement).value).toBe("Ada");
    expect(client.setDisplayName).not.toHaveBeenCalled();
    expect(client.signOut.mock.calls[0]).toEqual([]);
  });
});
