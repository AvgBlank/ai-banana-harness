import { chmod, mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { z } from "zod";
import type { Prefs } from "../shared/api";

const stored = z.object({ theme: z.enum(["system", "dark", "light"]).optional() });

export const DEFAULT_PREFS: Prefs = { theme: "dark" };

/** The user's preferences in one small JSON file. A missing or corrupt file means the defaults. */
export class Preferences {
  private current: Prefs | undefined;

  constructor(private readonly file: string) {}

  async get(): Promise<Prefs> {
    if (this.current) return this.current;
    let loaded: Prefs = { ...DEFAULT_PREFS };
    try {
      const parsed = stored.safeParse(JSON.parse(await readFile(this.file, "utf8")));
      if (parsed.success) loaded = { ...loaded, ...definedOnly(parsed.data) };
    } catch {
      // No file yet, or one that is not JSON: start from the defaults.
    }
    this.current = loaded;
    return loaded;
  }

  async set(patch: Partial<Prefs>): Promise<Prefs> {
    const next = { ...(await this.get()), ...definedOnly(patch) };
    // Written next to the target and renamed over it, so a crash leaves the old file intact.
    await mkdir(dirname(this.file), { recursive: true });
    const temp = `${this.file}.tmp`;
    await writeFile(temp, `${JSON.stringify(next, null, 2)}\n`, { mode: 0o600 });
    await chmod(temp, 0o600);
    await rename(temp, this.file);
    this.current = next;
    return next;
  }
}

/** Only the keys this version knows, and only those that are set. */
function definedOnly(value: Partial<Prefs>): Partial<Prefs> {
  return value.theme === undefined ? {} : { theme: value.theme };
}
