import type { AppInfo, KeyStatus, OpenedWorkspace } from "../../shared/api";
import { channels } from "../../shared/channels";
import { IpcError, ipcArgs, parseRpcCall } from "../../shared/ipc";
import { PROTOCOL_VERSION } from "../../shared/protocol";
import type { ExternalLinks } from "../external/links";
import type { KeyResolver } from "../key/resolver";
import type { LiveBuffers } from "../live/buffer";
import type { Preferences } from "../prefs";
import type { SidecarSupervisor } from "../sidecar/supervisor";
import { WorkspaceTrust } from "../trust/flow";
import type { TrustStore } from "../trust/store";
import type { ConfigFile } from "../trust/summary";
import type { KnownWorkspaces } from "./known";
import type { SenderEvent } from "./sender";
import { guarded, guardedNoArgs, type TrustContext } from "./validate";

export interface IpcPorts {
  handle(channel: string, listener: (event: SenderEvent, arg?: unknown) => unknown): void;
}

export interface IpcDeps {
  ipc: IpcPorts;
  trust: TrustContext;
  supervisor: Pick<SidecarSupervisor, "request" | "restart" | "setOpenWorkspaces" | "state">;
  keys: Pick<KeyResolver, "status" | "resolve" | "setForSession">;
  known: KnownWorkspaces;
  live: Pick<LiveBuffers, "attach" | "clear" | "started">;
  prefs: Pick<Preferences, "get" | "set" | "recordWorkspace" | "setDisplayName">;
  trustStore: TrustStore;
  /** Reads the workspace config for hashing; main's own reader unless a test supplies one. */
  readConfig?: (path: string) => Promise<ConfigFile>;
  links: ExternalLinks;
  appInfo: Omit<AppInfo, "protocol">;
  /** The system folder dialog; only main ever opens it. */
  pickFolder: () => Promise<string | null>;
}

export function registerIpc(deps: IpcDeps): void {
  const { ipc, trust, supervisor, known, links, keys } = deps;
  const opened = new Set<string>();
  const workspaces = new WorkspaceTrust({
    request: (method, params) => supervisor.request(method, params),
    store: deps.trustStore,
    readConfig: deps.readConfig,
    opened: (workspace, open) => {
      if (open) opened.add(workspace);
      else opened.delete(workspace);
      supervisor.setOpenWorkspaces([...opened]);
    },
  });
  // The renderer names a folder; only one main already knows can be opened, reviewed or trusted.
  const knownPath = (path: string) => {
    if (!known.has(path))
      throw new IpcError("unknown_workspace", "that folder was not chosen here");
  };

  ipc.handle(
    channels.appInfo,
    guardedNoArgs(trust, (): AppInfo => ({ ...deps.appInfo, protocol: PROTOCOL_VERSION })),
  );

  ipc.handle(
    channels.prefsGet,
    guardedNoArgs(trust, () => deps.prefs.get()),
  );

  ipc.handle(
    channels.prefsSet,
    guarded(trust, ipcArgs.prefsPatch, (patch) => deps.prefs.set(patch)),
  );

  ipc.handle(
    channels.profileSetDisplayName,
    guarded(trust, ipcArgs.profileName, ({ displayName }) =>
      deps.prefs.setDisplayName(displayName),
    ),
  );

  ipc.handle(
    channels.workspacePick,
    guardedNoArgs(trust, async () => {
      const path = await deps.pickFolder();
      if (path) known.add(path);
      return path;
    }),
  );

  ipc.handle(
    channels.workspaceOpen,
    guarded(trust, ipcArgs.workspacePath, async ({ path }): Promise<OpenedWorkspace> => {
      knownPath(path);
      const result = await workspaces.open(path);
      known.add(result.workspace);
      await deps.prefs.recordWorkspace(result.workspace).catch(() => undefined);
      return result;
    }),
  );

  ipc.handle(
    channels.trustReview,
    guarded(trust, ipcArgs.workspacePath, ({ path }) => {
      knownPath(path);
      return workspaces.review(path);
    }),
  );

  ipc.handle(
    channels.trustGrant,
    guarded(trust, ipcArgs.trustGrant, (request) => {
      knownPath(request.path);
      return workspaces.trust(request);
    }),
  );

  ipc.handle(
    channels.trustList,
    guardedNoArgs(trust, () => deps.trustStore.list()),
  );

  ipc.handle(
    channels.trustForget,
    guarded(trust, ipcArgs.workspacePath, ({ path }) => workspaces.forget(path)),
  );

  ipc.handle(
    channels.workspaceClose,
    guarded(trust, ipcArgs.workspacePath, async ({ path }) => {
      if (!opened.has(path)) throw new IpcError("unknown_workspace", "that workspace is not open");
      await supervisor.request("workspace.close", { workspace: path });
      deps.live.clear(path);
      opened.delete(path);
      supervisor.setOpenWorkspaces([...opened]);
    }),
  );

  ipc.handle(
    channels.keyStatus,
    guardedNoArgs(trust, (): KeyStatus => keys.status()),
  );

  // "Check again": the environment and the login shell once more. A key found while the
  // sidecar is waiting for one starts it over so it picks the key up.
  ipc.handle(
    channels.keyRecheck,
    guardedNoArgs(trust, async (): Promise<KeyStatus> => {
      const waiting = supervisor.state.state === "needs_key";
      const status = await keys.resolve(true);
      if (status.present && waiting) supervisor.restart();
      return status;
    }),
  );

  // The key is read once when the sidecar starts, so entering one starts it over. The reply
  // carries the status only.
  ipc.handle(
    channels.keySetForSession,
    guarded(trust, ipcArgs.keyForSession, ({ key }): KeyStatus => {
      const status = keys.setForSession(key);
      supervisor.restart();
      return status;
    }),
  );

  ipc.handle(
    channels.rpcCall,
    guarded(trust, ipcArgs.rpcCall, async (raw) => {
      const call = parseRpcCall(raw);
      const unmark =
        call.method === "turn.start"
          ? deps.live.started(call.params.workspace, call.params.session_id)
          : undefined;
      let result: unknown;
      try {
        result = await supervisor.request(call.method, call.params as never);
      } catch (error) {
        unmark?.();
        throw error;
      }
      if (call.method === "session.list") {
        known.learnFromSessions((result as { sessions: Array<{ workspace: string }> }).sessions);
      }
      return result;
    }),
  );

  ipc.handle(
    channels.externalOpen,
    guarded(trust, ipcArgs.externalOpen, ({ url }) => links.open(url)),
  );

  ipc.handle(
    channels.sidecarState,
    guardedNoArgs(trust, () => supervisor.state),
  );

  // One synchronous step in main: the snapshot of buffered frames and the running turn's id.
  ipc.handle(
    channels.liveAttach,
    guarded(trust, ipcArgs.liveAttach, ({ workspace, sessionId }) =>
      deps.live.attach(workspace, sessionId),
    ),
  );

  ipc.handle(
    channels.sidecarRestart,
    guardedNoArgs(trust, () => supervisor.restart()),
  );
}
