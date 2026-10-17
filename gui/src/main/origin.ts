import { realpathSync } from "node:fs";
import { join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { net, protocol } from "electron";

/** The renderer's own origin; nothing else is loaded into the window. */
export const APP_ORIGIN = "app://banana";
const SCHEME = "app";
const HOST = "banana";

/** Production policy: no inline script, no eval, no network, no frames, no forms. */
export const CONTENT_SECURITY_POLICY = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data:",
  "font-src 'self'",
  "worker-src 'self' blob:",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join("; ");

/** The Vite dev server needs an inline preamble and a socket for hot reload; dev only. */
export function devContentSecurityPolicy(devServer: string): string {
  const socket = devServer.replace(/^http/, "ws");
  return CONTENT_SECURITY_POLICY.replace(
    "script-src 'self'",
    "script-src 'self' 'unsafe-inline'",
  ).replace("connect-src 'self'", `connect-src 'self' ${devServer} ${socket}`);
}

/** Must run before the app is ready. */
export function registerAppScheme(): void {
  protocol.registerSchemesAsPrivileged([
    { scheme: SCHEME, privileges: { standard: true, secure: true, supportFetchAPI: true } },
  ]);
}

/**
 * The file a request path names inside `root`, or `null` for anything that escapes it. The path
 * is decoded once, then normalised; `..`, backslashes, NUL and symlinks out of the root are refused.
 */
export function resolveAppPath(root: string, urlPath: string): string | null {
  let decoded: string;
  try {
    decoded = decodeURIComponent(urlPath);
  } catch {
    return null;
  }
  if (decoded.includes("\0") || decoded.includes("\\")) return null;
  const relative = decoded === "/" || decoded === "" ? "index.html" : decoded.replace(/^\/+/, "");
  const base = resolve(root);
  const candidate = resolve(join(base, relative));
  if (candidate !== base && !candidate.startsWith(base + sep)) return null;
  try {
    const real = realpathSync(candidate);
    if (real !== realpathSync(base) && !real.startsWith(realpathSync(base) + sep)) return null;
    return real;
  } catch {
    return null;
  }
}

/** Serves `root` under `app://banana/` with the content security policy on every response. */
export function installAppProtocol(root: string): void {
  protocol.handle(SCHEME, async (request) => {
    const url = new URL(request.url);
    const file = url.host === HOST ? resolveAppPath(root, url.pathname) : null;
    if (!file) return new Response("not found", { status: 404 });
    const response = await net.fetch(pathToFileURL(file).toString());
    const headers = new Headers(response.headers);
    headers.set("Content-Security-Policy", CONTENT_SECURITY_POLICY);
    return new Response(response.body, { status: response.status, headers });
  });
}

/** Whether a navigation may happen at all: only inside the app origin (or the dev server). */
export function isAllowedNavigation(url: string, devServer: string | undefined): boolean {
  if (url.startsWith(`${APP_ORIGIN}/`) || url === APP_ORIGIN) return true;
  return Boolean(devServer && (url === devServer || url.startsWith(`${devServer}/`)));
}
