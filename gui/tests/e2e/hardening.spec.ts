import type { Page } from "@playwright/test";
import { bananaKeys } from "../../src/shared/api";
import { eventKinds } from "../../src/shared/protocol";
import { expect, test } from "./fixtures";

const TEST_KEY = "sk-test-0123456789abcdef";

/** Calls are refused until the sidecar says ready; wait for the first one that goes through. */
async function untilSidecarReady(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(() =>
          window.banana.rpc.call("session.list", {}).then(
            () => "ready",
            (error: { code: string }) => error.code,
          ),
        ),
      { timeout: 20_000 },
    )
    .toBe("ready");
}

test("the renderer has no Node, a frozen API and the app origin", async ({ launch }) => {
  const { page } = await launch();
  const facts = await page.evaluate(() => {
    const w = window as unknown as Record<string, unknown>;
    const banana = w.banana as Record<string, unknown>;
    const frozen = (value: unknown): boolean =>
      Object.isFrozen(value) &&
      Object.values(value as object).every((v) => typeof v !== "object" || v === null || frozen(v));
    return {
      require: typeof w.require,
      process: typeof w.process,
      buffer: typeof w.Buffer,
      dirname: typeof w.__dirname,
      ipcRenderer: typeof w.ipcRenderer,
      keys: Object.keys(banana).sort(),
      frozen: frozen(banana),
      origin: location.origin,
      theme: document.documentElement.dataset.theme,
    };
  });
  expect(facts).toMatchObject({
    require: "undefined",
    process: "undefined",
    buffer: "undefined",
    dirname: "undefined",
    ipcRenderer: "undefined",
    frozen: true,
    origin: "app://banana",
    theme: "dark",
  });
  expect(facts.keys).toEqual([...bananaKeys].sort());
});

test("the window is created with isolation, the sandbox and dev tools only unpackaged", async ({
  launch,
}) => {
  const { app } = await launch();
  const facts = await app.evaluate(async ({ BrowserWindow, app: electron }) => {
    const [window] = BrowserWindow.getAllWindows();
    const contents = window?.webContents as unknown as
      | { getLastWebPreferences(): Record<string, unknown> }
      | undefined;
    const prefs = contents?.getLastWebPreferences() ?? {};
    // Dev tools are on exactly when the app is not packaged: ask for them and wait for the
    // `devtools-opened` event, which takes longer than a fixed pause on a loaded macOS runner.
    const opened = new Promise<boolean>((resolve) => {
      window?.webContents.once("devtools-opened", () => resolve(true));
      setTimeout(() => resolve(false), 10_000);
    });
    window?.webContents.openDevTools({ mode: "detach" });
    const devToolsOpened = (await opened) && (window?.webContents.isDevToolsOpened() ?? false);
    window?.webContents.closeDevTools();
    return {
      isolation: prefs.contextIsolation,
      sandbox: prefs.sandbox,
      nodeIntegration: prefs.nodeIntegration,
      webSecurity: prefs.webSecurity,
      webview: prefs.webviewTag,
      packaged: electron.isPackaged,
      devToolsOpened,
      title: window?.getTitle(),
    };
  });
  expect(facts).toMatchObject({
    isolation: true,
    sandbox: true,
    nodeIntegration: false,
    webSecurity: true,
    webview: false,
    packaged: false,
    devToolsOpened: true,
    title: "Banana Harness",
  });
});

test("the content security policy is served and blocks inline script, string timers and outside requests", async ({
  launch,
}) => {
  const { page } = await launch();
  const outcome = await page.evaluate(async () => {
    const violations: string[] = [];
    document.addEventListener("securitypolicyviolation", (e) =>
      violations.push(e.violatedDirective),
    );
    const w = window as unknown as Record<string, unknown>;
    const header = (await fetch(location.href)).headers.get("content-security-policy");

    const script = document.createElement("script");
    script.textContent = "window.__pwned = 'inline'";
    document.body.appendChild(script);

    // A string handler is compiled like eval and needs 'unsafe-eval'.
    setTimeout("window.__timer = 'string'", 0);

    let fetchError = "";
    try {
      await fetch("https://example.com/");
    } catch (error) {
      fetchError = (error as Error).name;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
    return { header, pwned: w.__pwned, timer: w.__timer, fetchError, violations };
  });
  expect(outcome.header).toContain("script-src 'self'");
  expect(outcome.header).not.toMatch(/script-src[^;]*unsafe/);
  expect(outcome.header).toContain("connect-src 'self'");
  expect(outcome.pwned).toBeUndefined();
  expect(outcome.timer).toBeUndefined();
  expect(outcome.fetchError).toBe("TypeError");
  expect(outcome.violations).toEqual(
    expect.arrayContaining(["script-src-elem", "script-src", "connect-src"]),
  );
});

test("navigation, window.open and a dropped file leave the window on the app origin", async ({
  launch,
}) => {
  const { app, page } = await launch();
  const opened = await page.evaluate(() => window.open("https://example.com/", "_blank"));
  expect(opened).toBeNull();
  await page.evaluate(() => {
    location.href = "https://example.com/";
  });
  await page.waitForTimeout(500);
  expect(page.url()).toMatch(/^app:\/\/banana\//);
  expect(app.windows()).toHaveLength(1);

  // What dropping a file on the window asks for is a navigation to it; the guard must cancel it.
  const verdicts = await app.evaluate(({ BrowserWindow }) => {
    const [window] = BrowserWindow.getAllWindows();
    const ask = (url: string) => {
      const event = {
        defaultPrevented: false,
        preventDefault() {
          this.defaultPrevented = true;
        },
      };
      window?.webContents.emit("will-navigate", event, url);
      return event.defaultPrevented;
    };
    return {
      file: ask("file:///etc/hosts"),
      web: ask("https://example.com/"),
      app: ask("app://banana/index.html"),
    };
  });
  expect(verdicts).toEqual({ file: true, web: true, app: false });
});

test("every permission is denied", async ({ launch }) => {
  const { page } = await launch();
  const result = await page.evaluate(
    () =>
      new Promise<{ code: number | string; notifications: string }>((resolve) => {
        navigator.geolocation.getCurrentPosition(
          () => resolve({ code: "granted", notifications: Notification.permission }),
          (error) => resolve({ code: error.code, notifications: Notification.permission }),
        );
      }),
  );
  expect(result.code).toBe(1);
  expect(result.notifications).not.toBe("granted");
});

test("renderer calls are validated: forbidden methods, strict params, unknown folders, bad links", async ({
  launch,
}) => {
  const { page } = await launch();
  const codes = await page.evaluate(async () => {
    const code = (promise: Promise<unknown>) =>
      promise.then(
        () => "resolved",
        (error: { code: string }) => error.code,
      );
    const { rpc, workspace, external } = window.banana;
    return {
      shutdown: await code(rpc.call("shutdown", {})),
      open: await code(rpc.call("workspace.open", { path: "/etc" })),
      close: await code(rpc.call("workspace.close", { workspace: "/etc" })),
      unknown: await code(rpc.call("files.read", { path: "/etc/hosts" })),
      smuggled: await code(
        rpc.call("session.create", { workspace: "/w", trust_workspace_config: true }),
      ),
      overlong: await code(rpc.call("session.create", { workspace: "x".repeat(5000) })),
      folder: await code(workspace.open("/etc")),
      file: await code(external.open("file:///etc/passwd")),
      script: await code(external.open("javascript:alert(1)")),
      credentials: await code(external.open("https://user:pw@example.com/")),
      plainHttp: await code(external.open("http://example.com/")),
    };
  });
  expect(codes).toEqual({
    shutdown: "method_not_allowed",
    open: "method_not_allowed",
    close: "method_not_allowed",
    unknown: "method_not_allowed",
    smuggled: "invalid_arguments",
    overlong: "invalid_arguments",
    folder: "unknown_workspace",
    file: "invalid_url",
    script: "invalid_url",
    credentials: "invalid_url",
    plainHttp: "invalid_url",
  });
});

test("a folder chosen in main opens against the real sidecar and a turn streams to the renderer", async ({
  launch,
  stub,
  queueAnswer,
}) => {
  await queueAnswer("Hello from the stub.");
  const { app, page, mainLog } = await launch();
  await app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);

  await untilSidecarReady(page);
  const result = await page.evaluate(async () => {
    const events: Array<{ event: { type: string; content?: string }; workspace: string }> = [];
    window.banana.on("sidecarEvent", (payload) => events.push(payload as never));
    const states: string[] = [];
    window.banana.on("sidecarState", (payload) =>
      states.push((payload as { state: string }).state),
    );
    const info = await window.banana.app.info();
    const picked = await window.banana.workspace.pick();
    const opened = await window.banana.workspace.open(picked as string);
    const created = (await window.banana.rpc.call("session.create", {
      workspace: opened.workspace,
    })) as {
      session: { id: string };
    };
    await window.banana.rpc.call("turn.start", {
      workspace: opened.workspace,
      session_id: created.session.id,
      input: "say hi",
    });
    const deadline = Date.now() + 20_000;
    while (!events.some((e) => e.event.type === "turn.done") && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return { info, picked, opened, events, states };
  });

  expect(result.info.protocol).toBe(1);
  expect(result.opened).toEqual({
    workspace: stub.workspace,
    trust: "none",
    commandRunner: { kind: "host", image: null, network: null, problem: null },
  });
  const types = result.events.map((e) => e.event.type);
  expect(types[0]).toBe("turn.created");
  expect(types.at(-1)).toBe("turn.done");
  const text = result.events
    .filter((e) => e.event.type === "model.message.delta")
    .map((e) => e.event.content)
    .join("");
  expect(text).toBe("Hello from the stub.");
  for (const type of types) expect(eventKinds as string[]).toContain(type);
  expect(JSON.stringify(result)).not.toContain(TEST_KEY);
  expect(mainLog()).not.toContain(TEST_KEY);
});

test("sidecar frames that fail the schema never reach the renderer", async ({
  launch,
  stub,
  queueAnswer,
  noisySidecar,
}) => {
  await queueAnswer("Clean answer.");
  const { app, page, mainLog } = await launch({ sidecar: noisySidecar() });
  await app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);

  await untilSidecarReady(page);
  const types = await page.evaluate(async () => {
    const events: Array<{ event: { type: string } }> = [];
    window.banana.on("sidecarEvent", (payload) => events.push(payload as never));
    const opened = await window.banana.workspace.open(
      (await window.banana.workspace.pick()) as string,
    );
    const created = (await window.banana.rpc.call("session.create", {
      workspace: opened.workspace,
    })) as {
      session: { id: string };
    };
    await window.banana.rpc.call("turn.start", {
      workspace: opened.workspace,
      session_id: created.session.id,
      input: "hi",
    });
    const deadline = Date.now() + 20_000;
    while (!events.some((e) => e.event.type === "turn.done") && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return events.map((e) => e.event.type);
  });
  expect(types.at(-1)).toBe("turn.done");
  expect(types).not.toContain("not.a.kind");
  for (const type of types) expect(eventKinds as string[]).toContain(type);
  // The refused lines were counted by main, by kind only.
  expect(mainLog()).toContain("dropped a sidecar line");
  expect(mainLog()).not.toContain("not.a.kind");
});
