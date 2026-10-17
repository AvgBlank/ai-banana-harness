import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { join } from "node:path";
import { app, BrowserWindow, session } from "electron";
import { sidecarEnv } from "./sidecar/env";
import { SidecarSupervisor } from "./sidecar/supervisor";

// Set only by `electron-vite dev`; a packaged app has no dev server.
const devServer = process.env.ELECTRON_RENDERER_URL;

function createWindow(): void {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1100,
    minHeight: 720,
    // The title comes from the package metadata, not from the page.
    title: app.getName(),
    show: false,
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
    },
  });
  window.on("page-title-updated", (event) => event.preventDefault());
  window.once("ready-to-show", () => window.show());

  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event, url) => {
    if (!devServer || !url.startsWith(devServer)) event.preventDefault();
  });

  if (devServer) void window.loadURL(devServer);
  else void window.loadFile(join(__dirname, "../renderer/index.html"));
}

/** The bundled binary when packaged; in development `BANANA_HARNESS_BIN` or the debug build. */
function sidecarBinary(): string {
  if (app.isPackaged) return join(process.resourcesPath, "bin", "banana-harness");
  return process.env.BANANA_HARNESS_BIN ?? join(app.getAppPath(), "../target/debug/banana-harness");
}

function createSupervisor(): SidecarSupervisor {
  const supervisor = new SidecarSupervisor({
    bin: sidecarBinary(),
    spawn: (bin, args, options) =>
      spawn(bin, args, { env: options.env, stdio: "pipe" }) as ChildProcessWithoutNullStreams,
    // ponytail: the key comes from this process's environment only; the login-shell probe and
    // the in-memory entry arrive with the key flow.
    environment: () => ({
      variables: sidecarEnv(process.env, { apiKey: process.env.AI_API_KEY }),
      apiKey: process.env.AI_API_KEY,
    }),
    log: (message) => console.info(`sidecar: ${message}`),
  });
  supervisor.onState((state) =>
    console.info(`sidecar: ${state.state}${"reason" in state ? ` (${state.reason})` : ""}`),
  );
  return supervisor;
}

function main(): void {
  const supervisor = createSupervisor();

  app.whenReady().then(() => {
    session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) =>
      callback(false),
    );
    session.defaultSession.setPermissionCheckHandler(() => false);
    createWindow();
    supervisor.start();
    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });

  // A second launch focuses the first window, so there is never a second sidecar.
  app.on("second-instance", () => {
    const [window] = BrowserWindow.getAllWindows();
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
