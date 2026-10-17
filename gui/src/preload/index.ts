import { contextBridge, ipcRenderer } from "electron";
import type {
  AppInfo,
  BananaApi,
  KeyStatus,
  LiveAttach,
  OpenedWorkspace,
  Prefs,
  PrefsPatch,
  SidecarState,
  TrustedWorkspace,
  TrustReview,
} from "../shared/api";
import { channels, type PushName, pushChannels } from "../shared/channels";

type Envelope =
  | { ok: true; value: unknown }
  | { ok: false; error: { code: string; message: string } };

/** One invoke; a failure rejects with a plain `{ code, message }`, never an Error with a stack. */
async function invoke<T>(channel: string, arg?: unknown): Promise<T> {
  const result = (await ipcRenderer.invoke(channel, arg)) as Envelope;
  if (result.ok) return result.value as T;
  throw Object.freeze({ code: result.error.code, message: result.error.message });
}

const api: BananaApi = Object.freeze({
  app: Object.freeze({ info: () => invoke<AppInfo>(channels.appInfo) }),
  prefs: Object.freeze({
    get: () => invoke<Prefs>(channels.prefsGet),
    set: (patch: PrefsPatch) => invoke<Prefs>(channels.prefsSet, patch),
  }),
  workspace: Object.freeze({
    pick: () => invoke<string | null>(channels.workspacePick),
    open: (path: string) => invoke<OpenedWorkspace>(channels.workspaceOpen, { path }),
    close: (path: string) => invoke<void>(channels.workspaceClose, { path }),
    review: (path: string) => invoke<TrustReview>(channels.trustReview, { path }),
    trust: (request: { path: string; sha256: string; acknowledgeHosts: boolean }) =>
      invoke<OpenedWorkspace>(channels.trustGrant, request),
    trusted: () => invoke<TrustedWorkspace[]>(channels.trustList),
    forget: (path: string) => invoke<OpenedWorkspace | null>(channels.trustForget, { path }),
  }),
  key: Object.freeze({
    status: () => invoke<KeyStatus>(channels.keyStatus),
    recheck: () => invoke<KeyStatus>(channels.keyRecheck),
    setForSession: (key: string) => invoke<KeyStatus>(channels.keySetForSession, { key }),
  }),
  rpc: Object.freeze({
    call: (method: string, params: Record<string, unknown>) =>
      invoke<unknown>(channels.rpcCall, { method, params }),
  }),
  external: Object.freeze({ open: (url: string) => invoke<void>(channels.externalOpen, { url }) }),
  sidecar: Object.freeze({
    restart: () => invoke<void>(channels.sidecarRestart),
    state: () => invoke<SidecarState>(channels.sidecarState),
  }),
  live: Object.freeze({
    attach: (workspace: string, sessionId: string) =>
      invoke<LiveAttach>(channels.liveAttach, { workspace, sessionId }),
  }),
  on: (name: PushName, listener: (payload: unknown) => void) => {
    const channel = pushChannels[name];
    if (!channel) throw Object.freeze({ code: "unknown_event", message: "no such event" });
    // The listener gets the payload only, never the Electron event object.
    const wrapped = (_event: unknown, payload: unknown) => listener(payload);
    ipcRenderer.on(channel, wrapped);
    return () => {
      ipcRenderer.removeListener(channel, wrapped);
    };
  },
});

contextBridge.exposeInMainWorld("banana", api);
