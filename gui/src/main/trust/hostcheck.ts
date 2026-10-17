/** One provider's destination as the sidecar reports it (`provider.list`). */
export interface Destination {
  provider: string;
  host: string;
  scheme: string;
}

export type Verdict = { ok: true } | { ok: false; reason: string };

/** `scheme://authority`: what an approval is recorded as. */
export function originOf(destination: Pick<Destination, "scheme" | "host">): string {
  return `${destination.scheme}://${destination.host}`;
}

/** A loopback authority, with or without a port. */
export function isLoopback(authority: string): boolean {
  const host = authority.startsWith("[")
    ? authority.slice(0, authority.indexOf("]") + 1)
    : authority.split(":")[0];
  return host === "localhost" || host === "127.0.0.1" || host === "[::1]";
}

/**
 * Compares where each provider would send the API key with the baseline and the user's approvals.
 * A non-HTTPS endpoint that is not loopback is refused whatever was approved.
 */
export function checkDestinations(options: {
  baseline: Destination[];
  actual: Destination[];
  approved: string[];
}): Verdict {
  const { baseline, actual, approved } = options;
  for (const now of actual) {
    if (now.scheme !== "https" && !isLoopback(now.host)) {
      return { ok: false, reason: `${now.provider} would send the key over ${now.scheme}` };
    }
    const before = baseline.find((b) => b.provider === now.provider);
    const same = before && before.host === now.host && before.scheme === now.scheme;
    if (!same && !approved.includes(originOf(now))) {
      return { ok: false, reason: `${now.provider} would send the key to ${originOf(now)}` };
    }
  }
  return { ok: true };
}
