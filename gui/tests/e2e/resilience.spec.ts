import type { Launched, Stub } from "./fixtures";
import { expect, test } from "./fixtures";
import { alive, openFolder, reopenSession, sendPrompt, sidecarsOf } from "./support/ui";

const gated = (text: string, gate: string) => ({ sse: { text }, gate, gate_after_parts: 2 });
const release = (stub: Stub, gate: string) =>
  fetch(`${stub.stubs.baseline.control_url}/release/${gate}`, { method: "POST" });

/** The model call reached the stub: the turn is running for real, not only on screen. */
async function turnIsRunning(stub: Stub): Promise<void> {
  await expect
    .poll(
      async () =>
        (await (await fetch(`${stub.stubs.baseline.control_url}/requests`)).json()).length,
    )
    .toBeGreaterThan(0);
}

/** The sidecar is the one `serve --stdio` child of the app's main process. */
function sidecarPid(app: Launched): number {
  const pid = app.app.process().pid as number;
  const [child] = sidecarsOf(pid);
  if (child === undefined) throw new Error("the sidecar is not running");
  return child;
}

test("reload mid-stream: the turn is still running, the answer completes once", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies([gated("A slow answer.", "mid-stream")]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "answer slowly");
  await expect(app.page.getByRole("button", { name: "Stop" })).toBeVisible();

  await app.page.reload();
  await reopenSession(app.page, /answer slowly/);
  // Resumed and attached: still running, not interrupted.
  await expect(app.page.getByRole("button", { name: "Stop" })).toBeVisible();
  await expect(app.page.getByText("This turn was interrupted")).toBeHidden();

  await release(stub, "mid-stream");
  await expect(app.page.getByText("A slow answer.")).toBeVisible();
  await expect(app.page.getByText("A slow answer.")).toHaveCount(1);
  await expect(app.page.getByRole("button", { name: "Stop" })).toBeHidden();
});

test("reload mid-approval: the dialog comes back and the turn goes on after the answer", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies([
    {
      sse: {
        tool_calls: [
          { id: "c1", name: "write_file", arguments: { path: "resumed.txt", content: "hi\n" } },
        ],
      },
    },
    { sse: { text: "Wrote resumed.txt." } },
  ]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "make a file");
  await expect(app.page.getByRole("dialog").getByText("resumed.txt")).toBeVisible();

  await app.page.reload();
  await reopenSession(app.page, /make a file/);
  const dialog = app.page.getByRole("dialog");
  await expect(dialog.getByText("resumed.txt")).toBeVisible();
  await expect(dialog).toHaveCount(1);
  await dialog.getByRole("button", { name: "Allow once" }).click();
  await expect(app.page.getByText("Wrote resumed.txt.")).toBeVisible();
  await expect(dialog).toBeHidden();
});

test("a sidecar killed mid-turn: banner, interrupted turn, restart, nothing re-sent", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies([gated("A slow answer.", "never")]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "answer slowly");
  await expect(app.page.getByRole("button", { name: "Stop" })).toBeVisible();
  await turnIsRunning(stub);

  // The restart is quick: remember what the banner said rather than racing it.
  await app.page.evaluate(() => {
    const seen: string[] = [];
    (window as unknown as { __banners: string[] }).__banners = seen;
    new MutationObserver(() => {
      for (const alert of document.querySelectorAll('[role="alert"]'))
        if (alert.textContent && !seen.includes(alert.textContent)) seen.push(alert.textContent);
    }).observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  const before = sidecarPid(app);
  process.kill(before, "SIGKILL");

  await expect(app.page.getByText("This turn was interrupted")).toBeVisible();
  await expect
    .poll(() =>
      app.page.evaluate(() => (window as unknown as { __banners: string[] }).__banners.join("|")),
    )
    .toContain("Restarting… attempt 1 of 3");
  // Back to ready: a new process, the banner gone, the composer usable again.
  await expect.poll(() => sidecarsOf(app.app.process().pid as number)[0]).not.toBe(before);
  await expect(app.page.getByText(/Restarting… attempt/)).toBeHidden();
  await expect(app.page.getByLabel("Task")).toBeEnabled();
  await expect(app.page.getByText("This turn was interrupted")).toBeVisible();
  const seen = await (await fetch(`${stub.stubs.baseline.control_url}/requests`)).json();
  expect(seen).toHaveLength(1);
});

test("quit while a turn runs asks first; declining keeps it, confirming stops the sidecar", async ({
  launch,
  stub,
  queueReplies,
}) => {
  await queueReplies([gated("A slow answer.", "never")]);
  const app = await launch();
  await openFolder(app, stub);
  await sendPrompt(app.page, "answer slowly");
  await expect(app.page.getByRole("button", { name: "Stop" })).toBeVisible();
  await turnIsRunning(stub);
  const sidecar = sidecarPid(app);

  const answer = (response: number) =>
    app.app.evaluate(({ dialog }, wanted) => {
      const asked: string[] = [];
      (globalThis as unknown as { asked: string[] }).asked = asked;
      dialog.showMessageBox = (async (...args: unknown[]) => {
        const options = args.at(-1) as { message: string };
        asked.push(options.message);
        return { response: wanted, checkboxChecked: false };
      }) as never;
    }, response);

  // Cancel: the turn and the sidecar stay.
  await answer(1);
  await app.app.evaluate(({ app: electron }) => electron.quit());
  await expect
    .poll(() => app.app.evaluate(() => (globalThis as unknown as { asked: string[] }).asked))
    .toEqual(["A turn is running. Quit and cancel it?"]);
  await expect(app.page.getByRole("button", { name: "Stop" })).toBeVisible();
  expect(alive(sidecar)).toBe(true);

  // Quit: the app closes and takes the sidecar with it.
  await answer(0);
  const closed = app.app.waitForEvent("close");
  await app.app.evaluate(({ app: electron }) => electron.quit());
  await closed;
  await expect.poll(() => alive(sidecar), { timeout: 20_000 }).toBe(false);
});

test("killing the app leaves no orphan sidecar", async ({ launch }) => {
  const app = await launch();
  const sidecar = sidecarPid(app);
  process.kill(app.app.process().pid as number, "SIGKILL");
  // The sidecar sees its stdin close and exits on its own.
  await expect.poll(() => alive(sidecar), { timeout: 20_000 }).toBe(false);
});

test("a second launch focuses the first window and starts no second sidecar", async ({
  launch,
}) => {
  const first = await launch();
  const pid = first.app.process().pid as number;
  await first.app.evaluate(({ app: electron }) => {
    (globalThis as unknown as { second: boolean }).second = false;
    electron.on("second-instance", () => {
      (globalThis as unknown as { second: boolean }).second = true;
    });
  });
  const sidecars = sidecarsOf(pid);
  expect(sidecars).toHaveLength(1);

  // The second process finds the lock held and exits before any window opens.
  await expect(launch()).rejects.toThrow();
  await expect
    .poll(() => first.app.evaluate(() => (globalThis as unknown as { second: boolean }).second))
    .toBe(true);
  expect(sidecarsOf(pid)).toEqual(sidecars);
});

test("a crashed renderer is reloaded and the sidecar is untouched", async ({ launch }) => {
  const app = await launch();
  const sidecar = sidecarPid(app);
  // Playwright's page handle does not follow a crashed target, so the check runs in main.
  await app.app.evaluate(({ BrowserWindow }) => {
    const contents = BrowserWindow.getAllWindows()[0]?.webContents;
    const seen = { gone: 0, loads: 0 };
    (globalThis as unknown as { renderer: typeof seen }).renderer = seen;
    contents?.on("render-process-gone", () => seen.gone++);
    contents?.on("did-finish-load", () => seen.loads++);
    contents?.forcefullyCrashRenderer();
  });
  await expect
    .poll(
      () =>
        app.app.evaluate(async ({ BrowserWindow }) => {
          const contents = BrowserWindow.getAllWindows()[0];
          const seen = (globalThis as unknown as { renderer: { gone: number; loads: number } })
            .renderer;
          const drawn =
            seen.loads > 0 &&
            (await contents?.webContents
              .executeJavaScript('document.querySelector("nav") !== null')
              .catch(() => false));
          return { gone: seen.gone, drawn: Boolean(drawn) };
        }),
      { timeout: 20_000 },
    )
    .toEqual({ gone: 1, drawn: true });
  expect(alive(sidecar)).toBe(true);
  expect(sidecarPid(app)).toBe(sidecar);
});

test("a link the page opens asks with the full address, and a host can be trusted for the run", async ({
  launch,
}) => {
  const app = await launch();
  await app.app.evaluate(({ dialog, shell }) => {
    const state = { asked: [] as string[], opened: [] as string[], next: 1, trust: false };
    (globalThis as unknown as { links: typeof state }).links = state;
    dialog.showMessageBox = (async (...args: unknown[]) => {
      const options = args.at(-1) as { detail: string };
      state.asked.push(options.detail);
      return { response: state.next, checkboxChecked: state.trust };
    }) as never;
    shell.openExternal = (async (url: string) => {
      state.opened.push(url);
    }) as never;
  });
  const read = () =>
    app.app.evaluate(
      () => (globalThis as unknown as { links: { asked: string[]; opened: string[] } }).links,
    );
  const setAnswer = (next: number, trust: boolean) =>
    app.app.evaluate(
      (_e, a) => {
        const state = (globalThis as unknown as { links: { next: number; trust: boolean } }).links;
        state.next = a.next;
        state.trust = a.trust;
      },
      { next, trust },
    );
  const windows = () => app.app.windows().length;

  // Cancel: nothing opens, and no window appears.
  await app.page.evaluate(() => window.open("https://docs.example.com/a?b=c"));
  await expect.poll(async () => (await read()).asked).toEqual(["https://docs.example.com/a?b=c"]);
  expect((await read()).opened).toEqual([]);

  // Open and trust the host for this run: the next link to it is not asked about.
  await setAnswer(0, true);
  await app.page.evaluate(() => window.open("https://docs.example.com/b"));
  await expect.poll(async () => (await read()).opened).toEqual(["https://docs.example.com/b"]);
  await app.page.evaluate(() => window.open("https://docs.example.com/c"));
  await expect
    .poll(async () => (await read()).opened)
    .toEqual(["https://docs.example.com/b", "https://docs.example.com/c"]);
  expect((await read()).asked).toHaveLength(2);

  // A refused scheme never reaches the dialog.
  await app.page.evaluate(() => window.open("file:///etc/passwd"));
  await app.page.evaluate(() => window.open("https://other.example.com/"));
  await expect.poll(async () => (await read()).asked).toHaveLength(3);
  expect(windows()).toBe(1);
});
