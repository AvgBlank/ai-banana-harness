import { createHash } from "node:crypto";
import { lstat, readFile } from "node:fs/promises";
import { dirname } from "node:path";
import { parse } from "smol-toml";
import type { TrustSummary } from "../../shared/api";
import { type Destination, originOf } from "./hostcheck";

export const MAX_CONFIG_BYTES = 256 * 1024;

export type ConfigFile =
  | { ok: true; text: string; sha256: string }
  | { ok: false; reason: "missing" | "symlink" | "too_large" | "unreadable" };

/**
 * Reads the workspace config for hashing: a regular file (neither it nor its folder a symlink)
 * of at most 256 KiB. ponytail: the hash is taken just before the open, so a local attacker who
 * swaps the file in that window is outside the threat model.
 */
export async function readConfig(path: string): Promise<ConfigFile> {
  try {
    for (const target of [dirname(path), path]) {
      const info = await lstat(target);
      if (info.isSymbolicLink()) return { ok: false, reason: "symlink" };
      if (target === path) {
        if (!info.isFile()) return { ok: false, reason: "unreadable" };
        if (info.size > MAX_CONFIG_BYTES) return { ok: false, reason: "too_large" };
      }
    }
    const bytes = await readFile(path);
    if (bytes.length > MAX_CONFIG_BYTES) return { ok: false, reason: "too_large" };
    return {
      ok: true,
      text: bytes.toString("utf8"),
      sha256: createHash("sha256").update(bytes).digest("hex"),
    };
  } catch (error) {
    const missing = (error as NodeJS.ErrnoException).code === "ENOENT";
    return { ok: false, reason: missing ? "missing" : "unreadable" };
  }
}

type Table = Record<string, unknown>;
const isTable = (value: unknown): value is Table =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown): string => (typeof value === "string" ? value : String(value));

/**
 * An advisory reading of the config for the trust dialog. The raw text is always shown too, and
 * the destination check after the open does not depend on this parser. Returns `null` and the
 * parser's message when the TOML is invalid (the config will then fail to load).
 */
export function summarize(
  source: string,
  baseline: Destination[],
): { summary: TrustSummary; error: null } | { summary: null; error: string } {
  let config: Table;
  try {
    config = parse(source) as Table;
  } catch (error) {
    return { summary: null, error: error instanceof Error ? error.message : "invalid TOML" };
  }

  const mcp = Object.entries(isTable(config.mcp) ? config.mcp : {}).map(([name, raw]) => {
    const server = isTable(raw) ? raw : {};
    const env = isTable(server.env) ? server.env : {};
    const values = Object.values(env).map(text);
    return {
      name,
      command: Array.isArray(server.command) ? server.command.map(text) : [],
      envNames: Object.keys(env),
      leaksKey: values.some((v) => v.includes("{env:AI_API_KEY}")),
      usesEnvRef: values.some((v) => v.includes("{env:")),
      preload: server.preload === true,
    };
  });

  const permissions: TrustSummary["permissions"] = [];
  for (const [tool, raw] of Object.entries(isTable(config.permission) ? config.permission : {})) {
    const rules = isTable(raw) ? Object.entries(raw) : [[null, raw] as const];
    for (const [pattern, action] of rules) {
      const riskyTool = tool === "bash" || tool === "external_directory";
      permissions.push({
        tool,
        pattern,
        action: text(action),
        risky: text(action) === "allow" && riskyTool,
      });
    }
  }

  const providers: TrustSummary["providers"] = [];
  for (const [provider, raw] of Object.entries(isTable(config.providers) ? config.providers : {})) {
    const baseUrl = isTable(raw) && typeof raw.base_url === "string" ? raw.base_url : null;
    if (baseUrl === null) continue;
    let origin: string | null = null;
    try {
      const url = new URL(baseUrl);
      origin = originOf({ scheme: url.protocol.replace(/:$/, ""), host: url.host });
    } catch {
      // A URL that does not parse cannot be approved; the open will refuse it.
    }
    const before = baseline.find((b) => b.provider === provider);
    providers.push({
      provider,
      baseUrl,
      origin,
      differs: origin === null || !before || originOf(before) !== origin,
    });
  }

  const other: string[] = [];
  const agent = isTable(config.agent) ? config.agent : {};
  if ("instructions" in agent) other.push("agent.instructions");
  if ("tools" in config) other.push("tools");
  if (isTable(config.skills) && "paths" in config.skills) other.push("skills.paths");

  return { summary: { mcp, permissions, providers, other }, error: null };
}
