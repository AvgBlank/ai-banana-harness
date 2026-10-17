import { chmod, mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { z } from "zod";
import type { Prefs, PrefsPatch } from "../shared/api";

export const MAX_RECENTS = 10;
const path = z.string().min(1).max(4096);

const stored = z.object({
  theme: z.enum(["system", "dark", "light"]).optional(),
  sidebarOpen: z.boolean().optional(),
  changesOpen: z.boolean().optional(),
  changesScope: z.enum(["turn", "workspace"]).optional(),
  changesWidth: z.number().int().min(360).max(760).optional(),
  lastWorkspace: path.nullable().optional(),
  recents: z.array(path).optional(),
});

export const DEFAULT_PREFS: Prefs = {
  theme: "dark",
  sidebarOpen: true,
  changesOpen: false,
  changesScope: "turn",
  changesWidth: 480,
  lastWorkspace: null,
  recents: [],
};

/** The user's preferences in one small JSON file. A missing or corrupt file means the defaults. */
export class Preferences {
  private current: Prefs | undefined;

  constructor(private readonly file: string) {}

  async get(): Promise<Prefs> {
    if (this.current) return this.current;
    let loaded: Prefs = { ...DEFAULT_PREFS };
    try {
      const parsed = stored.safeParse(JSON.parse(await readFile(this.file, "utf8")));
      if (parsed.success) loaded = normalise({ ...loaded, ...definedOnly(parsed.data) });
    } catch {
      // No file yet, or one that is not JSON: start from the defaults.
    }
    this.current = loaded;
    return loaded;
  }

  /** What the renderer may change: appearance and panel state. */
  async set(patch: PrefsPatch): Promise<Prefs> {
    const { theme, sidebarOpen, changesOpen, changesScope, changesWidth } = patch;
    return this.save({
      ...(await this.get()),
      ...definedOnly({ theme, sidebarOpen, changesOpen, changesScope, changesWidth }),
    });
  }

  /** Main notes a workspace it opened: the last one, and the newest of the recent ten. */
  async recordWorkspace(workspace: string): Promise<Prefs> {
    const now = await this.get();
    const recents = [workspace, ...now.recents.filter((p) => p !== workspace)];
    return this.save({ ...now, lastWorkspace: workspace, recents });
  }

  private async save(next: Prefs): Promise<Prefs> {
    const value = normalise(next);
    // Written next to the target and renamed over it, so a crash leaves the old file intact.
    await mkdir(dirname(this.file), { recursive: true });
    const temp = `${this.file}.tmp`;
    await writeFile(temp, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
    await chmod(temp, 0o600);
    await rename(temp, this.file);
    this.current = value;
    return value;
  }
}

function normalise(prefs: Prefs): Prefs {
  return { ...prefs, recents: [...new Set(prefs.recents)].slice(0, MAX_RECENTS) };
}

/** Only the keys this version knows, and only those that are set. */
function definedOnly<T extends object>(value: T): Partial<T> {
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined)) as Partial<T>;
}
