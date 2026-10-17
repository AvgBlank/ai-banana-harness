import type {
  AppInfo,
  BananaApi,
  KeyStatus,
  OpenedWorkspace,
  Prefs,
  PrefsPatch,
} from "../../shared/api";
import type {
  EventFrame,
  MethodName,
  Params,
  ProviderOption,
  Result,
  Session,
} from "../../shared/protocol";

/** A reply to a required action, as the sidecar takes it. */
export type Answer = "allow" | "allow_for_session" | "deny" | { text: string };

/** The sidecar state main pushes, as far as the shell reads it. */
export type SidecarStatus = { state: string };

/**
 * Everything the renderer asks of the outside world, in one typed place. Components get it from
 * context, so a test supplies a fake and no component reaches for `window.banana`.
 */
export interface Client {
  appInfo(): Promise<AppInfo>;
  prefs(): Promise<Prefs>;
  setPrefs(patch: PrefsPatch): Promise<Prefs>;
  keyStatus(): Promise<KeyStatus>;
  recheckKey(): Promise<KeyStatus>;
  /** Hands a typed key to main, which keeps it in memory and restarts the sidecar with it. */
  setSessionKey(key: string): Promise<KeyStatus>;
  listSessions(): Promise<Session[]>;
  /** Session ids with a turn running now. */
  runningSessions(): Promise<string[]>;
  renameSession(workspace: string, sessionId: string, title: string): Promise<Session>;
  providers(workspace: string): Promise<{ providers: ProviderOption[]; default: string }>;
  createSession(workspace: string, provider?: string): Promise<Session>;
  /** The session with its stored turns. */
  resume(workspace: string, sessionId: string): Promise<Result<"session.resume">>;
  startTurn(workspace: string, sessionId: string, input: string): Promise<void>;
  cancelTurn(workspace: string, sessionId: string): Promise<void>;
  replyAction(
    workspace: string,
    sessionId: string,
    actionId: string,
    answer: Answer,
  ): Promise<void>;
  /** Ids of the required actions the sidecar is waiting on, across open workspaces. */
  pendingActionIds(): Promise<string[]>;
  switchProvider(workspace: string, sessionId: string, provider: string): Promise<Session>;
  /** Every event frame main forwards, already validated. */
  onSidecarEvent(listener: (frame: EventFrame) => void): () => void;
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
    keyStatus: () => banana.key.status(),
    recheckKey: () => banana.key.recheck(),
    setSessionKey: (key) => banana.key.setForSession(key),
    listSessions: async () => (await call("session.list", {})).sessions,
    runningSessions: async () =>
      (await call("workspace.list", {})).workspaces.flatMap((w) =>
        w.active_turn ? [w.active_turn.session_id] : [],
      ),
    renameSession: async (workspace, session_id, title) =>
      (await call("session.rename", { workspace, session_id, title })).session,
    providers: (workspace) => call("provider.list", { workspace }),
    createSession: async (workspace, provider) =>
      (
        await call("session.create", {
          workspace,
          ...(provider ? { provider: provider as "deepseek" | "qwen" } : {}),
        })
      ).session,
    resume: (workspace, session_id) => call("session.resume", { workspace, session_id }),
    startTurn: async (workspace, session_id, input) => {
      await call("turn.start", { workspace, session_id, input });
    },
    cancelTurn: async (workspace, session_id) => {
      await call("turn.cancel", { workspace, session_id });
    },
    replyAction: async (workspace, session_id, action_id, answer) => {
      await call("action.reply", { workspace, session_id, action_id, answer });
    },
    pendingActionIds: async () =>
      (await call("workspace.list", {})).workspaces.flatMap(
        (w) => w.active_turn?.pending_action_ids ?? [],
      ),
    switchProvider: async (workspace, session_id, provider) =>
      (
        await call("session.switch_provider", {
          workspace,
          session_id,
          provider: provider as "deepseek" | "qwen",
        })
      ).session,
    onSidecarEvent: (listener) =>
      banana.on("sidecarEvent", (payload) => listener(payload as EventFrame)),
    pickFolder: () => banana.workspace.pick(),
    openWorkspace: (path) => banana.workspace.open(path),
    onSidecarState: (listener) =>
      banana.on("sidecarState", (payload) => listener(payload as SidecarStatus)),
  };
}
