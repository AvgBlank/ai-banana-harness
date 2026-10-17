import type { AppInfo, KeyStatus, OpenedWorkspace } from "../../shared/api";
import { channels } from "../../shared/channels";
import { IpcError, ipcArgs, parseRpcCall } from "../../shared/ipc";
import { PROTOCOL_VERSION } from "../../shared/protocol";
import type { ExternalLinks } from "../external/links";
import type { KeyResolver } from "../key/resolver";
import type { Preferences } from "../prefs";
import type { SidecarSupervisor } from "../sidecar/supervisor";
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
  prefs: Pick<Preferences, "get" | "set" | "recordWorkspace">;
  links: ExternalLinks;
  appInfo: Omit<AppInfo, "protocol">;
  /** The system folder dialog; only main ever opens it. */
  pickFolder: () => Promise<string | null>;
}

export function registerIpc(deps: IpcDeps): void {
  const { ipc, trust, supervisor, known, links, keys } = deps;
  const opened = new Set<string>();

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
      if (!known.has(path))
        throw new IpcError("unknown_workspace", "that folder was not chosen here");
      // Always restricted: the renderer never supplies the trust flag.
      const result = await supervisor.request("workspace.open", {
        path,
        trust_workspace_config: false,
      });
      known.add(result.workspace);
      opened.add(result.workspace);
      supervisor.setOpenWorkspaces([...opened]);
      await deps.prefs.recordWorkspace(result.workspace).catch(() => undefined);
      return { workspace: result.workspace, restricted: !result.workspace_config.loaded };
    }),
  );

  ipc.handle(
    channels.workspaceClose,
    guarded(trust, ipcArgs.workspacePath, async ({ path }) => {
      if (!opened.has(path)) throw new IpcError("unknown_workspace", "that workspace is not open");
      await supervisor.request("workspace.close", { workspace: path });
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
      const result = await supervisor.request(call.method, call.params as never);
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
    channels.sidecarRestart,
    guardedNoArgs(trust, () => supervisor.restart()),
  );
}
