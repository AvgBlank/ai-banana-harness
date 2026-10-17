import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, test } from "vitest";
import { App } from "@/App";
import { ClientProvider } from "@/state/client-context";
import { DEFAULT_PREFS, fakeClient, reviewOf, session } from "./fakes";

const WS = "/w";
const proxyReview = () =>
  reviewOf(WS, {
    summary: {
      mcp: [],
      permissions: [],
      providers: [
        {
          provider: "deepseek",
          baseUrl: "https://proxy.example",
          origin: "https://proxy.example",
          differs: true,
        },
      ],
      other: [],
    },
  });

async function open(options: Parameters<typeof fakeClient>[0] = {}) {
  const client = fakeClient({
    sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
    picked: WS,
    trust: { [WS]: "needs_review" },
    ...options,
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
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Open folder" })));
  await act(async () => undefined);
  return client;
}

describe("the trust dialog", () => {
  test("a folder with an untrusted config opens the dialog with restricted mode focused", async () => {
    const client = await open();
    const dialog = await screen.findByRole("dialog");
    expect(client.reviewTrust).toHaveBeenCalledWith(WS);
    expect(document.activeElement).toBe(
      within(dialog).getByRole("button", { name: "Open in restricted mode" }),
    );
    // MCP programs with the full command, env names and the key-leak warning.
    expect(within(dialog).getByText("tool --flag")).toBeTruthy();
    expect(within(dialog).getByText(/Environment: TOKEN/)).toBeTruthy();
    expect(within(dialog).getByText(/Passes your API key/)).toBeTruthy();
    expect(within(dialog).getByText(/sudo \*/)).toBeTruthy();
    expect(within(dialog).getByText("agent.instructions")).toBeTruthy();
    expect(within(dialog).getByText(/AGENTS.md and skills still guide/)).toBeTruthy();
  });

  test("restricted mode closes it, shows the chip, and Review and trust reopens it", async () => {
    const client = await open();
    await act(async () =>
      fireEvent.click(
        within(await screen.findByRole("dialog")).getByRole("button", {
          name: "Open in restricted mode",
        }),
      ),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("Restricted")).toBeTruthy();
    expect(client.trustWorkspace).not.toHaveBeenCalled();
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Review and trust…" })),
    );
    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(client.reviewTrust).toHaveBeenCalledTimes(2);
  });

  test("the raw file can be viewed", async () => {
    await open();
    const dialog = await screen.findByRole("dialog");
    await act(async () => fireEvent.click(within(dialog).getByText("View raw file")));
    expect((within(dialog).getByLabelText("Raw config") as HTMLTextAreaElement).value).toContain(
      "[mcp.x]",
    );
  });

  test("trusting sends the reviewed hash and closes the dialog", async () => {
    const client = await open();
    const dialog = await screen.findByRole("dialog");
    await act(async () =>
      fireEvent.click(within(dialog).getByRole("button", { name: "Trust and open" })),
    );
    expect(client.trustWorkspace).toHaveBeenCalledWith({
      path: WS,
      sha256: "c".repeat(64),
      acknowledgeHosts: false,
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.queryByText("Restricted")).toBeNull();
  });

  test("a new destination needs the checkbox before Trust and open works", async () => {
    const client = await open({ review: proxyReview() });
    const dialog = await screen.findByRole("dialog");
    const trust = within(dialog).getByRole("button", {
      name: "Trust and open",
    }) as HTMLButtonElement;
    expect(trust.disabled).toBe(true);
    expect(within(dialog).getByText(/Differs from https:\/\/api.deepseek.com/)).toBeTruthy();
    await act(async () =>
      fireEvent.click(
        within(dialog).getByLabelText("My API key will be sent to https://proxy.example"),
      ),
    );
    expect(trust.disabled).toBe(false);
    await act(async () => fireEvent.click(trust));
    expect(client.trustWorkspace).toHaveBeenCalledWith(
      expect.objectContaining({ acknowledgeHosts: true }),
    );
  });

  test("a config that does not parse shows the warning and cannot be trusted", async () => {
    await open({ review: reviewOf(WS, { parseError: "bad toml", summary: null }) });
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("alert").textContent).toContain("bad toml");
    expect(
      (within(dialog).getByRole("button", { name: "Trust and open" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  test("a refusal such as a running turn is shown in the dialog and the dialog stays", async () => {
    const client = await open();
    client.trustWorkspace.mockRejectedValueOnce({
      code: "turn_running",
      message: "stop the running turn before changing trust",
    });
    const dialog = await screen.findByRole("dialog");
    await act(async () =>
      fireEvent.click(within(dialog).getByRole("button", { name: "Trust and open" })),
    );
    expect((await within(dialog).findByRole("alert")).textContent).toContain("running turn");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  test("a blocked config closes the dialog, tells the user, and leaves the workspace unopened", async () => {
    const client = await open();
    client.trustWorkspace.mockRejectedValueOnce({
      code: "blocked_config",
      message: "Blocked: this folder's config changes where your API key is sent.",
    });
    await act(async () =>
      fireEvent.click(
        within(await screen.findByRole("dialog")).getByRole("button", { name: "Trust and open" }),
      ),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect((await screen.findByRole("alert")).textContent).toContain("Blocked:");
  });

  test("an open that main blocks shows the notice and no dialog", async () => {
    const client = fakeClient({
      sessions: [session({ id: "s1", workspace: WS, title: "Fix it" })],
      picked: WS,
    });
    client.openWorkspace.mockRejectedValue({ code: "blocked_config", message: "Blocked: moved." });
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
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Open folder" })));
    expect((await screen.findByRole("alert")).textContent).toBe("Blocked: moved.Dismiss");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("a trusted or config-free folder shows no dialog and no chip", async () => {
    await open({ trust: { [WS]: "trusted" } });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText("Restricted")).toBeNull();
  });

  test("an unreadable config shows the chip without a dialog", async () => {
    await open({ trust: { [WS]: "unreadable" } });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("Restricted")).toBeTruthy();
  });
});

describe("trusted workspaces in settings", () => {
  test("each trusted workspace can be forgotten, which puts it back to restricted", async () => {
    const client = await open({
      trust: { [WS]: "trusted" },
      trusted: [
        { workspace: WS, trustedAt: "2026-10-17T10:00:00Z", approvedHosts: ["https://p.example"] },
      ],
    });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /Settings/ })));
    const list = await screen.findByRole("list", { name: "Trusted workspaces" });
    expect(within(list).getByText(/https:\/\/p.example/)).toBeTruthy();
    client.trustedWorkspaces.mockResolvedValue([]);
    await act(async () => fireEvent.click(within(list).getByRole("button", { name: "Forget w" })));
    expect(client.forgetTrust).toHaveBeenCalledWith(WS);
    expect(await screen.findByText("No workspace is trusted.")).toBeTruthy();
  });

  test("a refusal is shown and the entry stays listed", async () => {
    const client = await open({
      trust: { [WS]: "trusted" },
      trusted: [{ workspace: WS, trustedAt: "2026-10-17T10:00:00Z", approvedHosts: [] }],
    });
    client.forgetTrust.mockRejectedValueOnce({
      code: "turn_running",
      message: "stop the turn first",
    });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /Settings/ })));
    await act(async () => fireEvent.click(await screen.findByRole("button", { name: "Forget w" })));
    expect((await screen.findByRole("alert")).textContent).toContain("stop the turn first");
    expect(screen.getByRole("button", { name: "Forget w" })).toBeTruthy();
  });
});
