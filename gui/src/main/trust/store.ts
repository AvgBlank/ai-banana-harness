import { chmod, mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { z } from "zod";
import type { TrustedWorkspace } from "../../shared/api";

const entry = z.object({
  config_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  approved_hosts: z.array(z.string().max(512)),
  trusted_at: z.string().max(64),
});
const file = z.object({ version: z.literal(1), workspaces: z.record(z.string(), entry) });

type Stored = z.output<typeof file>;

export interface TrustEntry {
  configSha256: string;
  /** Destinations (`scheme://authority`) the user approved when trusting this file. */
  approvedHosts: string[];
}

/**
 * Which workspace config files the user trusted, bound to their content. It lives in the app's
 * data folder only; a workspace and the sidecar never see it.
 */
export class TrustStore {
  private queue: Promise<unknown> = Promise.resolve();

  constructor(
    private readonly path: string,
    private readonly now: () => Date = () => new Date(),
  ) {}

  async get(workspace: string): Promise<TrustEntry | null> {
    const found = (await this.load()).workspaces[workspace];
    return found
      ? { configSha256: found.config_sha256, approvedHosts: found.approved_hosts }
      : null;
  }

  async list(): Promise<TrustedWorkspace[]> {
    return Object.entries((await this.load()).workspaces)
      .map(([workspace, e]) => ({
        workspace,
        trustedAt: e.trusted_at,
        approvedHosts: e.approved_hosts,
      }))
      .sort((a, b) => a.workspace.localeCompare(b.workspace));
  }

  trust(workspace: string, value: TrustEntry): Promise<void> {
    return this.change((stored) => {
      stored.workspaces[workspace] = {
        config_sha256: value.configSha256,
        approved_hosts: value.approvedHosts,
        trusted_at: this.now().toISOString(),
      };
    });
  }

  forget(workspace: string): Promise<void> {
    return this.change((stored) => {
      delete stored.workspaces[workspace];
    });
  }

  /** Changes run one at a time, so two quick edits never overwrite each other. */
  private change(edit: (stored: Stored) => void): Promise<void> {
    const run = async () => {
      const stored = await this.load();
      edit(stored);
      // Written next to the target and renamed over it, so a crash leaves the old file intact.
      await mkdir(dirname(this.path), { recursive: true });
      const temp = `${this.path}.tmp`;
      await writeFile(temp, `${JSON.stringify(stored, null, 2)}\n`, { mode: 0o600 });
      await chmod(temp, 0o600);
      await rename(temp, this.path);
    };
    const next = this.queue.then(run, run);
    this.queue = next.catch(() => undefined);
    return next;
  }

  private async load(): Promise<Stored> {
    let text: string;
    try {
      text = await readFile(this.path, "utf8");
    } catch {
      return { version: 1, workspaces: {} };
    }
    try {
      const parsed = file.safeParse(JSON.parse(text));
      if (parsed.success) return parsed.data;
    } catch {
      // Not JSON: handled with the other kind of corruption below.
    }
    // Nothing is trusted from a file that cannot be read; it is kept aside for the user.
    await rename(this.path, `${this.path}.corrupt-${this.now().getTime()}`).catch(() => undefined);
    return { version: 1, workspaces: {} };
  }
}
