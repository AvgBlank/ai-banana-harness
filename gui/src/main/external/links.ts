import { IpcError } from "../../shared/ipc";

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]"]);

/**
 * A link the user may be asked about: `https:` anywhere, `http:` only on loopback (OAuth
 * redirects), no embedded credentials. Everything else (`file:`, `javascript:`, `data:`,
 * custom schemes) is refused.
 */
export function parseExternalUrl(raw: string): URL {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new IpcError("invalid_url", "the link is not a valid URL");
  }
  const loopbackHttp = url.protocol === "http:" && LOOPBACK.has(url.hostname);
  if (url.protocol !== "https:" && !loopbackHttp) {
    throw new IpcError("invalid_url", "only https links can be opened");
  }
  if (url.username || url.password) {
    throw new IpcError("invalid_url", "links with credentials are not opened");
  }
  return url;
}

export interface LinkPorts {
  /** Shows the full URL; resolves with whether to open it and whether to stop asking for the host. */
  confirm: (url: URL) => Promise<{ open: boolean; trustHost: boolean }>;
  open: (url: string) => Promise<void>;
}

/** Validates, confirms (unless the host was approved this session), then opens in the browser. */
export class ExternalLinks {
  private readonly approvedHosts = new Set<string>();

  constructor(private readonly ports: LinkPorts) {}

  async open(raw: string): Promise<void> {
    const url = parseExternalUrl(raw);
    if (!this.approvedHosts.has(url.host)) {
      const answer = await this.ports.confirm(url);
      if (!answer.open) return;
      if (answer.trustHost) this.approvedHosts.add(url.host);
    }
    await this.ports.open(url.toString());
  }
}
