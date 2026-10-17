import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { join } from "node:path";
import { app, BrowserWindow, dialog, ipcMain, shell } from "electron";
import { pushChannels } from "../shared/channels";
import { authLinkOf, ExternalLinks } from "./external/links";
import { KnownWorkspaces } from "./ipc/known";
import { registerIpc } from "./ipc/register";
import { probeLoginShell } from "./key/probe";
import { KeyResolver } from "./key/resolver";
import { LiveBuffers } from "./live/buffer";
import { installAppProtocol, registerAppScheme } from "./origin";
import { Preferences } from "./prefs";
import { QuitGuard } from "./quit";
import { SidecarSupervisor } from "./sidecar/supervisor";
import { TrustStore } from "./trust/store";
import { createWindow, denyPermissions, devServer } from "./window";

// Privileges for the `app://` scheme must be registered before the app is ready.
registerAppScheme();

/** The bundled binary when packaged; in development `BANANA_HARNESS_BIN` or the debug build. */
function sidecarBinary(): string {
  if (app.isPackaged) return join(process.resourcesPath, "bin", "banana-harness");
  return process.env.BANANA_HARNESS_BIN ?? join(app.getAppPath(), "../target/debug/banana-harness");
}

function createSupervisor(keys: KeyResolver): SidecarSupervisor {
  const supervisor: SidecarSupervisor = new SidecarSupervisor({
    bin: sidecarBinary(),
    spawn: (bin, args, options) =>
      spawn(bin, args, { env: options.env, stdio: "pipe" }) as ChildProcessWithoutNullStreams,
    environment: () => keys.environment(),
    // Restricted again; the renderer opens the workspace anew once the sidecar is ready, which
    // re-evaluates trust before anything stronger.
    reopenWorkspace: async (path) => {
      await supervisor.request("workspace.open", { path, trust_workspace_config: false });
    },
    log: (message) => console.info(`sidecar: ${message}`),
  });
  supervisor.onState((state) =>
    console.info(`sidecar: ${state.state}${"reason" in state ? ` (${state.reason})` : ""}`),
  );
  return supervisor;
}

function main(): void {
  const keys = new KeyResolver({
    env: process.env,
    probe: () => probeLoginShell(process.env.SHELL),
  });
  const prefs = new Preferences(join(app.getPath("userData"), "preferences.json"));
  const known = new KnownWorkspaces();
  const live = new LiveBuffers();
  const trustStore = new TrustStore(join(app.getPath("userData"), "trust.json"));
  const supervisor = createSupervisor(keys);
  let window: BrowserWindow | null = null;

  const toWindow = (channel: string, payload: unknown) => {
    if (window && !window.isDestroyed()) window.webContents.send(channel, payload);
  };
  supervisor.onState((state) => toWindow(pushChannels.sidecarState, state));
  // Only frames that passed the schema reach this point.
  supervisor.onEvent((frame) => {
    live.push(frame);
    toWindow(pushChannels.sidecarEvent, frame);
    // A remote MCP server's login address goes through the same confirmed path as any link.
    const login = authLinkOf(frame);
    if (login) void links.open(login).catch(() => undefined);
  });
  // A turn that died with the process leaves nothing to catch up on.
  supervisor.onState((state) => {
    if (state.state !== "ready" && state.state !== "starting" && state.state !== "needs_key") {
      live.clear();
    }
  });

  const links = new ExternalLinks({
    confirm: async (url) => {
      const options = {
        type: "question" as const,
        buttons: ["Open", "Cancel"],
        defaultId: 1,
        cancelId: 1,
        message: "Open this link in your browser?",
        detail: url.toString(),
        checkboxLabel: `Do not ask again for ${url.host} until the app restarts`,
      };
      const answer = window
        ? await dialog.showMessageBox(window, options)
        : await dialog.showMessageBox(options);
      return { open: answer.response === 0, trustHost: answer.checkboxChecked };
    },
    open: (url) => shell.openExternal(url),
  });

  const openExternal = (url: string) => links.open(url);

  app.whenReady().then(async () => {
    installAppProtocol(join(__dirname, "../renderer"));
    denyPermissions();
    registerIpc({
      ipc: ipcMain,
      trust: {
        topFrame: () => window?.webContents.mainFrame ?? null,
        devServer,
        log: (message) => console.info(`ipc: ${message}`),
      },
      supervisor,
      known,
      live,
      keys,
      prefs,
      trustStore,
      links,
      appInfo: { name: app.getName(), version: app.getVersion(), platform: process.platform },
      pickFolder: async () => {
        const options = { properties: ["openDirectory" as const] };
        const result = window
          ? await dialog.showOpenDialog(window, options)
          : await dialog.showOpenDialog(options);
        return result.canceled ? null : (result.filePaths[0] ?? null);
      },
    });
    // The workspaces main opened before are known again, so a recent one opens like a new pick.
    const saved = await prefs.get();
    for (const path of [...saved.recents, ...(saved.lastWorkspace ? [saved.lastWorkspace] : [])]) {
      known.add(path);
    }
    // The key is found before the sidecar starts, so it starts with it.
    await keys.resolve();
    window = createWindow(openExternal);
    supervisor.start();
    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) window = createWindow(openExternal);
    });
  });

  // A second launch focuses the first window, so there is never a second sidecar.
  app.on("second-instance", () => {
    if (!window) return;
    if (window.isMinimized()) window.restore();
    window.focus();
  });

  const quitGuard = new QuitGuard({
    turnRunning: () => live.running,
    confirm: async () => {
      const options = {
        type: "question" as const,
        buttons: ["Quit", "Cancel"],
        defaultId: 1,
        cancelId: 1,
        message: "A turn is running. Quit and cancel it?",
        detail: "Files it already changed stay; you can undo from the session.",
      };
      const answer = window
        ? await dialog.showMessageBox(window, options)
        : await dialog.showMessageBox(options);
      return answer.response === 0;
    },
    shutdown: (force) => supervisor.shutdown(force),
    quit: () => app.quit(),
  });
  app.on("before-quit", (event) => {
    if (!quitGuard.handle()) event.preventDefault();
  });

  app.on("window-all-closed", () => {
    if (process.platform !== "darwin") app.quit();
  });
}

if (app.requestSingleInstanceLock()) main();
else app.quit();
