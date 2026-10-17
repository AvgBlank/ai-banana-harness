import type { AppInfo, BananaApi, OpenedWorkspace, Prefs } from "../../shared/api";
import type { MethodName, Params, ProviderOption, Result, Session } from "../../shared/protocol";

/** The sidecar state main pushes, as far as the shell reads it. */
export type SidecarStatus = { state: string };

/**
 * Everything the renderer asks of the outside world, in one typed place. Components get it from
 * context, so a test supplies a fake and no component reaches for `window.banana`.
 */
export interface Client {
  appInfo(): Promise<AppInfo>;
  prefs(): Promise<Prefs>;
  setPrefs(patch: Partial<Prefs>): Promise<Prefs>;
  listSessions(): Promise<Session[]>;
  /** Session ids with a turn running now. */
  runningSessions(): Promise<string[]>;
  renameSession(workspace: string, sessionId: string, title: string): Promise<Session>;
  providers(workspace: string): Promise<{ providers: ProviderOption[]; default: string }>;
  pickFolder(): Promise<string | null>;
  openWorkspace(path: string): Promise<OpenedWorkspace>;
  onSidecarState(listener: (status: SidecarStatus) => void): () => void;
}

export function clientFor(banana: BananaApi): Client {
  const call = <M extends MethodName>(method: M, params: Params<M>) =>
    banana.rpc.call(method, params as Record<string, unknown>) as Promise<Result<M>>;
  return {
    appInfo: () => banana.app.info(),
    prefs: () => banana.prefs.get(),
    setPrefs: (patch) => banana.prefs.set(patch),
    listSessions: async () => (await call("session.list", {})).sessions,
    runningSessions: async () =>
      (await call("workspace.list", {})).workspaces.flatMap((w) =>
        w.active_turn ? [w.active_turn.session_id] : [],
      ),
    renameSession: async (workspace, session_id, title) =>
      (await call("session.rename", { workspace, session_id, title })).session,
    providers: (workspace) => call("provider.list", { workspace }),
    pickFolder: () => banana.workspace.pick(),
    openWorkspace: (path) => banana.workspace.open(path),
    onSidecarState: (listener) =>
      banana.on("sidecarState", (payload) => listener(payload as SidecarStatus)),
  };
}
