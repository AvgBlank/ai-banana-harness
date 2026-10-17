import type { PushName } from "./channels";

export interface AppInfo {
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
export const bananaKeys = ["app", "workspace", "rpc", "external", "sidecar", "on"] as const;
