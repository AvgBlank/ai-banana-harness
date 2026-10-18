import type { PushName } from "./channels";
import type { EventFrame } from "./protocol";

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

/** Automatic restarts the supervisor allows in a minute before it waits for the user. */
export const MAX_RESTARTS = 3;

export type FailureReason =
  | "no_ready"
  | "incompatible_protocol"
  | "fatal"
  | "exited_early"
  | "restart_budget";

/** Where the sidecar is: the supervisor's state, and what main tells the renderer. */
export type SidecarState =
  | { state: "starting" }
  | { state: "ready"; reopened: string[] }
  | { state: "needs_key" }
  | { state: "crashed"; attempt: number; restartInMs: number }
  | { state: "failed"; reason: FailureReason; message: string }
  | { state: "stopping" }
  | { state: "stopped" };

export interface LiveAttach {
  frames: EventFrame[];
  activeTurnId: string | null;
}

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

/**
 * What the workspace config allows. `none`: there is no config to trust. `needs_review`: there is
 * one and the user has not trusted this exact file. `unreadable`: it cannot be hashed (a symlink,
 * too large, unreadable) or fails to load, so the workspace stays restricted.
 */
export type TrustState = "none" | "trusted" | "needs_review" | "unreadable";

/**
 * Where a workspace's `bash` commands run. `problem` says why a container cannot run a command
 * now (Docker missing or stopped, image missing) and what to do; it is `null` when it can.
 */
export interface CommandRunnerInfo {
  kind: "host" | "container";
  image: string | null;
  network: "none" | "bridge" | null;
  problem: string | null;
}

/** The result of opening a workspace: restricted until the trust flow upgrades it. */
export interface OpenedWorkspace {
  workspace: string;
  trust: TrustState;
  commandRunner: CommandRunnerInfo;
}

/** An advisory reading of a workspace config for the trust dialog. */
export interface TrustSummary {
  mcp: Array<{
    name: string;
    command: string[];
    envNames: string[];
    /** An env value holds `{env:AI_API_KEY}`. */
    leaksKey: boolean;
    /** An env value expands some environment variable. */
    usesEnvRef: boolean;
    preload: boolean;
  }>;
  permissions: Array<{ tool: string; pattern: string | null; action: string; risky: boolean }>;
  providers: Array<{
    provider: string;
    baseUrl: string;
    /** `scheme://authority`, or `null` when the URL does not parse. */
    origin: string | null;
    /** The destination differs from the baseline. */
    differs: boolean;
  }>;
  /** Overrides of the agent's instructions, tools or skill paths. */
  other: string[];
  /** Settings only the user's own config can make (where commands run); the workspace's are ignored. */
  ignored: string[];
}

/** Everything the trust dialog shows, read by main from the file itself. */
export interface TrustReview {
  workspace: string;
  configPath: string;
  /** Hash of the text shown; trusting is bound to it. */
  sha256: string;
  raw: string;
  /** Set when the TOML does not parse; then there is no summary. */
  parseError: string | null;
  summary: TrustSummary | null;
  /** Where each provider sends the key before the config applies. */
  baseline: Array<{ provider: string; origin: string }>;
}

export interface TrustedWorkspace {
  workspace: string;
  trustedAt: string;
  approvedHosts: string[];
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
    /** Main reads the workspace config and returns it with its summary. */
    review(path: string): Promise<TrustReview>;
    /** Trusts the file whose hash was reviewed and reopens the workspace with it. */
    trust(request: {
      path: string;
      sha256: string;
      acknowledgeHosts: boolean;
    }): Promise<OpenedWorkspace>;
    trusted(): Promise<TrustedWorkspace[]>;
    /** Removes the trust entry; an open workspace goes back to restricted (`null` when closed). */
    forget(path: string): Promise<OpenedWorkspace | null>;
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
  readonly sidecar: {
    restart(): Promise<void>;
    /** Where the sidecar is now; later changes arrive as the `sidecarState` push. */
    state(): Promise<SidecarState>;
  };
  readonly live: {
    /** The buffered frames of the session's current or last turn, and its running turn id. */
    attach(workspace: string, sessionId: string): Promise<LiveAttach>;
  };
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
  "live",
  "on",
] as const;
