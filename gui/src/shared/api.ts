import type { PushName } from "./channels";

export type Theme = "system" | "dark" | "light";

/** What main persists for the user. */
export interface Prefs {
  theme: Theme;
}

export interface AppInfo {
  /** The product name, from the package metadata; the interface shows it, never spells it. */
  name: string;
  version: string;
  platform: string;
  protocol: number;
}

/** The result of opening a workspace: always restricted until the trust flow upgrades it. */
export interface OpenedWorkspace {
  workspace: string;
  restricted: boolean;
}

/** The only object the preload exposes to the renderer as `window.banana`. */
export interface BananaApi {
  readonly app: { info(): Promise<AppInfo> };
  readonly prefs: {
    get(): Promise<Prefs>;
    /** Merges a patch and returns the stored result; unknown keys are dropped. */
    set(patch: Partial<Prefs>): Promise<Prefs>;
  };
  readonly workspace: {
    /** The system folder dialog; `null` when cancelled. */
    pick(): Promise<string | null>;
    /** Opens a folder main already knows (picked this run, or a session's workspace). */
    open(path: string): Promise<OpenedWorkspace>;
  };
  readonly rpc: { call(method: string, params: Record<string, unknown>): Promise<unknown> };
  readonly external: { open(url: string): Promise<void> };
  readonly sidecar: { restart(): Promise<void> };
  /** Subscribes to a main-to-renderer push; returns the unsubscribe function. */
  on(name: PushName, listener: (payload: unknown) => void): () => void;
}

/** The keys of `window.banana`; a new capability is added here and nowhere else. */
export const bananaKeys = [
  "app",
  "prefs",
  "workspace",
  "rpc",
  "external",
  "sidecar",
  "on",
] as const;
