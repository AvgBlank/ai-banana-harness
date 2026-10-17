import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { join } from "node:path";
import { app, BrowserWindow, dialog, ipcMain, shell } from "electron";
import { pushChannels } from "../shared/channels";
import { ExternalLinks } from "./external/links";
import { KnownWorkspaces } from "./ipc/known";
import { registerIpc } from "./ipc/register";
import { installAppProtocol, registerAppScheme } from "./origin";
import { Preferences } from "./prefs";
import { sidecarEnv } from "./sidecar/env";
import { SidecarSupervisor } from "./sidecar/supervisor";
import { createWindow, denyPermissions, devServer } from "./window";

// Privileges for the `app://` scheme must be registered before the app is ready.
registerAppScheme();

/** The bundled binary when packaged; in development `BANANA_HARNESS_BIN` or the debug build. */
function sidecarBinary(): string {
  if (app.isPackaged) return join(process.resourcesPath, "bin", "banana-harness");
  return process.env.BANANA_HARNESS_BIN ?? join(app.getAppPath(), "../target/debug/banana-harness");
}

function createSupervisor(): SidecarSupervisor {
  const supervisor: SidecarSupervisor = new SidecarSupervisor({
    bin: sidecarBinary(),
    spawn: (bin, args, options) =>
      spawn(bin, args, { env: options.env, stdio: "pipe" }) as ChildProcessWithoutNullStreams,
    // ponytail: the key comes from this process's environment only; the login-shell probe and
    // the in-memory entry arrive with the key flow.
    environment: () => ({
      variables: sidecarEnv(process.env, { apiKey: process.env.AI_API_KEY }),
      apiKey: process.env.AI_API_KEY,
    }),
    // Restricted again; the trust flow re-evaluates trust before anything stronger.
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
  const supervisor = createSupervisor();
  let window: BrowserWindow | null = null;

  const toWindow = (channel: string, payload: unknown) => {
    if (window && !window.isDestroyed()) window.webContents.send(channel, payload);
  };
  supervisor.onState((state) => toWindow(pushChannels.sidecarState, state));
  // Only frames that passed the schema reach this point.
  supervisor.onEvent((frame) => toWindow(pushChannels.sidecarEvent, frame));

  app.whenReady().then(() => {
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
      known: new KnownWorkspaces(),
      prefs: new Preferences(join(app.getPath("userData"), "preferences.json")),
      links: new ExternalLinks({
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
      }),
      appInfo: { name: app.getName(), version: app.getVersion(), platform: process.platform },
      pickFolder: async () => {
        const options = { properties: ["openDirectory" as const] };
        const result = window
          ? await dialog.showOpenDialog(window, options)
          : await dialog.showOpenDialog(options);
        return result.canceled ? null : (result.filePaths[0] ?? null);
      },
    });
    window = createWindow();
    supervisor.start();
    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) window = createWindow();
    });
  });

  // A second launch focuses the first window, so there is never a second sidecar.
  app.on("second-instance", () => {
    if (!window) return;
    if (window.isMinimized()) window.restore();
    window.focus();
  });

  // Quit stops the sidecar first; a second quit while that runs takes the kill path.
  let stopping = false;
  let stopped = false;
  app.on("before-quit", (event) => {
    if (stopped) return;
    event.preventDefault();
    if (stopping) {
      void supervisor.shutdown(true);
      return;
    }
    stopping = true;
    void supervisor.shutdown().finally(() => {
      stopped = true;
      app.quit();
    });
  });

  app.on("window-all-closed", () => {
    if (process.platform !== "darwin") app.quit();
  });
}

if (app.requestSingleInstanceLock()) main();
else app.quit();
