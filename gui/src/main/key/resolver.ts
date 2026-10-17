import { sidecarEnv } from "../sidecar/env";
import type { ProbeResult } from "./probe";

export type KeySource = "env" | "login_shell" | "session" | "none";

/** All the renderer ever learns about the key. */
export interface KeyStatus {
  present: boolean;
  source: KeySource;
}

/** What launchd gives a Dock-launched app; a sign the real `PATH` lives in the user's shell. */
const LAUNCHD_PATH = "/usr/bin:/bin:/usr/sbin:/sbin";

export interface ResolverOptions {
  /** The app's own environment. */
  env: Record<string, string | undefined>;
  probe: () => Promise<ProbeResult | null>;
}

/**
 * Finds the API key in order: the app's environment, then the user's login shell, then a
 * memory-only entry. The value lives here and in the sidecar's environment, nowhere else.
 */
export class KeyResolver {
  private key: string | undefined;
  private source: KeySource = "none";
  private shell: ProbeResult = {};
  private entered: string | undefined;
  private resolved = false;

  constructor(private readonly options: ResolverOptions) {}

  /** The first resolution at startup, and "Check again" (`force`), which ignores what was found. */
  async resolve(force = false): Promise<KeyStatus> {
    if (this.resolved && !force) return this.status();
    this.resolved = true;
    const { env } = this.options;
    const fromEnv = clean(env.AI_API_KEY);
    this.shell = {};
    const needsPath = !env.PATH || env.PATH === LAUNCHD_PATH;
    if (!fromEnv || needsPath) this.shell = (await this.options.probe()) ?? {};

    if (fromEnv) this.set(fromEnv, "env");
    else if (clean(this.shell.AI_API_KEY)) this.set(clean(this.shell.AI_API_KEY), "login_shell");
    else if (this.entered) this.set(this.entered, "session");
    else this.set(undefined, "none");
    return this.status();
  }

  /** The user typed a key. It is kept in memory for this run and never written anywhere. */
  setForSession(key: string): KeyStatus {
    const trimmed = clean(key);
    if (!trimmed) return this.status();
    this.entered = trimmed;
    this.set(trimmed, "session");
    return this.status();
  }

  status(): KeyStatus {
    return { present: this.key !== undefined, source: this.source };
  }

  /** The sidecar's whole environment, with the key and the shell's `PATH` and proxy variables. */
  environment(): { variables: Record<string, string>; apiKey?: string } {
    const { env } = this.options;
    const proxies = Object.fromEntries(
      Object.entries(this.shell).filter(
        ([name]) => /proxy$/i.test(name) && env[name] === undefined,
      ),
    );
    const variables = sidecarEnv(
      { ...env, ...proxies },
      { path: this.shell.PATH ?? env.PATH, apiKey: this.key },
    );
    return { variables, apiKey: this.key };
  }

  private set(key: string | undefined, source: KeySource): void {
    this.key = key;
    this.source = key ? source : "none";
  }
}

function clean(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}
