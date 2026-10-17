const EXACT = new Set([
  "HOME",
  "USER",
  "LOGNAME",
  "SHELL",
  "TMPDIR",
  "LANG",
  "XDG_CONFIG_HOME",
  "XDG_DATA_HOME",
  "SSL_CERT_FILE",
  "SSL_CERT_DIR",
]);
const PROXY = /^(http_proxy|https_proxy|all_proxy|no_proxy)$/i;

/**
 * The sidecar's whole environment: this allowlist, the resolved `PATH` and the key. Everything
 * else (`ELECTRON_*`, `NODE_OPTIONS`, ...) is dropped. The key lives here and nowhere else.
 */
export function sidecarEnv(
  source: Record<string, string | undefined>,
  resolved: { path?: string; apiKey?: string },
): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [name, value] of Object.entries(source)) {
    if (value === undefined) continue;
    if (EXACT.has(name) || name.startsWith("LC_") || PROXY.test(name)) env[name] = value;
  }
  const path = resolved.path ?? source.PATH;
  if (path) env.PATH = path;
  if (resolved.apiKey) env.AI_API_KEY = resolved.apiKey;
  return env;
}

/** `text` with every occurrence of the key replaced. An empty key redacts nothing. */
export function redact(text: string, apiKey: string | undefined): string {
  return apiKey ? text.split(apiKey).join("[REDACTED]") : text;
}
