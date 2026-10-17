// IPC channel names; kept free of imports so the sandboxed preload bundles only this.

export const channels = {
  appInfo: "banana:app:info",
  prefsGet: "banana:prefs:get",
  prefsSet: "banana:prefs:set",
  workspacePick: "banana:workspace:pick",
  workspaceOpen: "banana:workspace:open",
  workspaceClose: "banana:workspace:close",
  trustReview: "banana:trust:review",
  trustGrant: "banana:trust:grant",
  trustList: "banana:trust:list",
  trustForget: "banana:trust:forget",
  keyStatus: "banana:key:status",
  keyRecheck: "banana:key:recheck",
  keySetForSession: "banana:key:set-for-session",
  rpcCall: "banana:rpc:call",
  externalOpen: "banana:external:open",
  sidecarRestart: "banana:sidecar:restart",
} as const;

/** Main to renderer, one channel each. */
export const pushChannels = {
  sidecarState: "banana:sidecar:state",
  sidecarEvent: "banana:sidecar:event",
} as const;

export type PushName = keyof typeof pushChannels;
