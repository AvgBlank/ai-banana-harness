import { join } from "node:path";
import { app, BrowserWindow, session } from "electron";
import { APP_ORIGIN, devContentSecurityPolicy, isAllowedNavigation } from "./origin";

/** Set only by `electron-vite dev`; a packaged app has no dev server. */
export const devServer = process.env.ELECTRON_RENDERER_URL;

/** Denies every permission request: the app needs none (camera, microphone, location, ...). */
export function denyPermissions(): void {
  session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) =>
    callback(false),
  );
  session.defaultSession.setPermissionCheckHandler(() => false);
  if (devServer) {
    const policy = devContentSecurityPolicy(devServer);
    session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
      callback({
        responseHeaders: { ...details.responseHeaders, "Content-Security-Policy": [policy] },
      });
    });
  }
}

/** `openExternal` takes a link the page asked to open; it validates and confirms it itself. */
export function createWindow(openExternal: (url: string) => Promise<void>): BrowserWindow {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1100,
    minHeight: 720,
    // The title comes from the package metadata, not from the page.
    title: app.getName(),
    show: false,
    // The Codex-style title bar: content runs under it and macOS draws the traffic lights.
    titleBarStyle: "hiddenInset",
    trafficLightPosition: { x: 14, y: 14 },
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      webviewTag: false,
      devTools: !app.isPackaged,
    },
  });
  window.on("page-title-updated", (event) => event.preventDefault());
  window.once("ready-to-show", () => window.show());

  const { webContents } = window;
  // Never a new window; a link the page opens goes through the confirmed external-link path.
  webContents.setWindowOpenHandler(({ url }) => {
    void openExternal(url).catch(() => undefined);
    return { action: "deny" };
  });
  // The sidecar is untouched by a renderer crash: reload, and the page catches up from main.
  // ponytail: no limit on repeated crashes; cap the reloads per minute if one ever loops.
  webContents.on("render-process-gone", (_event, details) => {
    if (details.reason !== "clean-exit" && !window.isDestroyed()) webContents.reload();
  });
  const guard = (event: { preventDefault(): void }, url: string) => {
    if (!isAllowedNavigation(url, devServer)) event.preventDefault();
  };
  webContents.on("will-navigate", guard);
  webContents.on("will-redirect", guard);

  void window.loadURL(devServer ?? `${APP_ORIGIN}/index.html`);
  return window;
}
