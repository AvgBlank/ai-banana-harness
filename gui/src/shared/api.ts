import type { PushName } from "./channels";

export type Theme = "system" | "dark" | "light";

/** What main persists for the user. */
export interface Prefs {
  theme: Theme;
  sidebarOpen: boolean;
  changesOpen: boolean;
  /** Which changes the panel shows, and how wide it is (pixels). */
  changesScope: "turn" | "workspace";
  changesWidth: number;
  /** The workspace main opened last, and up to ten recent ones, newest first. Main owns both. */
  lastWorkspace: string | null;
  recents: string[];
}

/** The part of the preferences the renderer may change. */
export type PrefsPatch = Partial<
  Pick<Prefs, "theme" | "sidebarOpen" | "changesOpen" | "changesScope" | "changesWidth">
>;

export type KeySource = "env" | "login_shell" | "session" | "none";

/** All the renderer is told about the API key: whether there is one and where it came from. */
export interface KeyStatus {
  present: boolean;
  source: KeySource;
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
    set(patch: PrefsPatch): Promise<Prefs>;
  };
  readonly workspace: {
    /** The system folder dialog; `null` when cancelled. */
    pick(): Promise<string | null>;
    /** Opens a folder main already knows (picked this run, or a session's workspace). */
    open(path: string): Promise<OpenedWorkspace>;
    close(path: string): Promise<void>;
  };
  readonly key: {
    status(): Promise<KeyStatus>;
    /** Checks the environment and the login shell again, ignoring what was found before. */
    recheck(): Promise<KeyStatus>;
    /** Keeps a typed key in memory for this run and restarts the sidecar with it; write-only. */
    setForSession(key: string): Promise<KeyStatus>;
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
  "key",
  "rpc",
  "external",
  "sidecar",
  "on",
] as const;
