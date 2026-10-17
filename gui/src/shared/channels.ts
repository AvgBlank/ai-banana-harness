// IPC channel names; kept free of imports so the sandboxed preload bundles only this.

export const channels = {
  appInfo: "banana:app:info",
  workspacePick: "banana:workspace:pick",
  workspaceOpen: "banana:workspace:open",
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
