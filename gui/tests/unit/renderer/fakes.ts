import { type Mocked, vi } from "vitest";
import type { Client, SidecarStatus } from "../../../src/renderer/lib/client";
import type { FileChange } from "../../../src/renderer/state/changes";
import type {
  KeyStatus,
  Prefs,
  PrefsPatch,
  TrustedWorkspace,
  TrustReview,
  TrustState,
} from "../../../src/shared/api";
import type { EventFrame, ProviderOption, Session } from "../../../src/shared/protocol";

export function session(over: Partial<Session> & Pick<Session, "id" | "workspace">): Session {
  return {
    title: `Session ${over.id}`,
    provider: "deepseek",
    model: "deepseek-flash",
    created_at: "2026-10-17T10:00:00.000Z",
    updated_at: "2026-10-17T10:00:00.000Z",
    ...over,
  };
}

export function fileChange(over: Partial<FileChange> & Pick<FileChange, "path">): FileChange {
  return {
    status: "modified",
    added: 1,
    removed: 1,
    patch: "--- a\n+++ b\n@@ -1,1 +1,1 @@\n-old\n+new\n",
    truncated: false,
    ...over,
  };
}

export function reviewOf(workspace: string, over: Partial<TrustReview> = {}): TrustReview {
  return {
    workspace,
    configPath: `${workspace}/.banana-harness/config.toml`,
    sha256: "c".repeat(64),
    raw: "[mcp.x]\ncommand = ['tool']\n",
    parseError: null,
    summary: {
      mcp: [
        {
          name: "x",
          command: ["tool", "--flag"],
          envNames: ["TOKEN"],
          leaksKey: true,
          usesEnvRef: true,
          preload: true,
        },
      ],
      permissions: [{ tool: "bash", pattern: "sudo *", action: "allow", risky: true }],
      providers: [],
      other: ["agent.instructions"],
    },
    baseline: [{ provider: "deepseek", origin: "https://api.deepseek.com" }],
    ...over,
  };
}

export const DEFAULT_PREFS: Prefs = {
  theme: "dark",
  sidebarOpen: true,
  changesOpen: false,
  changesScope: "turn" as const,
  changesWidth: 480,
  lastWorkspace: null,
  recents: [],
};

export const PROVIDERS: ProviderOption[] = [
  { provider: "deepseek", model: "deepseek-flash", host: "api.deepseek.com", scheme: "https" },
  { provider: "qwen", model: "qwen3.8-flash", host: "dashscope.aliyuncs.com", scheme: "https" },
];

export type FakeClient = Mocked<Client> & {
  pushState(status: SidecarStatus): void;
  /** Delivers a frame as main would forward it. */
  pushEvent(frame: EventFrame): void;
};

export function frame(
  sessionId: string,
  turnId: string | null,
  event: { id: string; type: string; [k: string]: unknown },
  workspace = "/w",
): EventFrame {
  return { kind: "event", workspace, session_id: sessionId, turn_id: turnId, event } as EventFrame;
}

/** An in-memory stand-in for the preload API; every method is a spy. */
export function fakeClient(
  options: {
    sessions?: Session[];
    turns?: Record<string, unknown[]>;
    /** What `workspace.list` reports as waiting. */
    listedPending?: string[];
    running?: string[];
    prefs?: Prefs;
    picked?: string | null;
    key?: KeyStatus;
    foundOnRecheck?: KeyStatus;
    turnFiles?: FileChange[];
    workspaceFiles?: FileChange[];
    /** The trust state `workspace.open` reports, by path (`none` otherwise). */
    trust?: Record<string, TrustState>;
    review?: TrustReview;
    trusted?: TrustedWorkspace[];
  } = {},
): FakeClient {
  const listeners = new Set<(status: SidecarStatus) => void>();
  const created: Session[] = [];
  const eventListeners = new Set<(frame: EventFrame) => void>();
  const stored: Prefs = { ...DEFAULT_PREFS, ...options.prefs };
  let key: KeyStatus = options.key ?? { present: true, source: "env" };
  const client = {
    appInfo: vi.fn(async () => ({
      name: "Test App",
      version: "0.1.0",
      platform: "darwin",
      protocol: 1,
    })),
    prefs: vi.fn(async () => stored),
    setPrefs: vi.fn(async (patch: PrefsPatch) => Object.assign(stored, patch)),
    keyStatus: vi.fn(async () => key),
    recheckKey: vi.fn(async () => {
      key = options.foundOnRecheck ?? key;
      return key;
    }),
    setSessionKey: vi.fn(async (_typed: string) => {
      key = { present: true, source: "session" };
      return key;
    }),
    listSessions: vi.fn(async () => [...created, ...(options.sessions ?? [])]),
    runningSessions: vi.fn(async () => options.running ?? []),
    renameSession: vi.fn(async (_workspace: string, id: string, title: string) =>
      session({ id, workspace: "/w", title }),
    ),
    providers: vi.fn(async () => ({ providers: PROVIDERS, default: "deepseek" })),
    pickFolder: vi.fn(async () => options.picked ?? null),
    openWorkspace: vi.fn(async (path: string) => ({
      workspace: path,
      trust: options.trust?.[path] ?? ("none" as const),
    })),
    reviewTrust: vi.fn(async (path: string) => options.review ?? reviewOf(path)),
    trustWorkspace: vi.fn(async (request: { path: string }) => ({
      workspace: request.path,
      trust: "trusted" as const,
    })),
    trustedWorkspaces: vi.fn(async () => options.trusted ?? []),
    forgetTrust: vi.fn(async (path: string) => ({
      workspace: path,
      trust: "needs_review" as const,
    })),
    onSidecarState: vi.fn((listener: (status: SidecarStatus) => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    }),
    createSession: vi.fn(async (workspace: string) => {
      const made = session({ id: "new1", workspace, title: null });
      created.push(made);
      return made;
    }),
    resume: vi.fn(async (_workspace: string, id: string) => ({
      session: session({ id, workspace: "/w" }),
      turns: options.turns?.[id] ?? [],
    })),
    startTurn: vi.fn(async (_w: string, _s: string, _i: string) => undefined),
    cancelTurn: vi.fn(async (_w: string, _s: string) => undefined),
    replyAction: vi.fn(async (..._args: unknown[]) => undefined),
    pendingActionIds: vi.fn(async () => options.listedPending ?? []),
    switchProvider: vi.fn(async (workspace: string, id: string, provider: string) =>
      session({
        id,
        workspace,
        provider,
        model: provider === "qwen" ? "qwen3.8-flash" : "deepseek-flash",
      }),
    ),
    changesTurn: vi.fn(async (_w: string, _s: string) => ({
      checkpoint: true,
      files: options.turnFiles ?? [],
    })),
    changesWorkspace: vi.fn(async (_w: string) => ({
      git: true,
      files: options.workspaceFiles ?? [],
    })),
    undo: vi.fn(async (_w: string, _s: string) => ({
      outcome: "restored" as const,
      event: { id: "undo1", type: "turn.undone", turn_id: "T1" },
    })),
    onSidecarEvent: vi.fn((listener: (f: EventFrame) => void) => {
      eventListeners.add(listener);
      return () => eventListeners.delete(listener);
    }),
    pushEvent: (f: EventFrame) => {
      for (const listener of eventListeners) listener(f);
    },
    pushState: (status: SidecarStatus) => {
      for (const listener of listeners) listener(status);
    },
  };
  return client as unknown as FakeClient;
}
