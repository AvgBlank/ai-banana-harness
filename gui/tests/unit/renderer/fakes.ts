import { type Mocked, vi } from "vitest";
import type { Client, SidecarStatus } from "../../../src/renderer/lib/client";
import type { KeyStatus, Prefs, PrefsPatch } from "../../../src/shared/api";
import type { ProviderOption, Session } from "../../../src/shared/protocol";

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

export const DEFAULT_PREFS: Prefs = {
  theme: "dark",
  sidebarOpen: true,
  changesOpen: false,
  lastWorkspace: null,
  recents: [],
};

export const PROVIDERS: ProviderOption[] = [
  { provider: "deepseek", model: "deepseek-flash", host: "api.deepseek.com", scheme: "https" },
  { provider: "qwen", model: "qwen3.8-flash", host: "dashscope.aliyuncs.com", scheme: "https" },
];

export type FakeClient = Mocked<Client> & { pushState(status: SidecarStatus): void };

/** An in-memory stand-in for the preload API; every method is a spy. */
export function fakeClient(
  options: {
    sessions?: Session[];
    running?: string[];
    prefs?: Prefs;
    picked?: string | null;
    key?: KeyStatus;
    foundOnRecheck?: KeyStatus;
  } = {},
): FakeClient {
  const listeners = new Set<(status: SidecarStatus) => void>();
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
    listSessions: vi.fn(async () => options.sessions ?? []),
    runningSessions: vi.fn(async () => options.running ?? []),
    renameSession: vi.fn(async (_workspace: string, id: string, title: string) =>
      session({ id, workspace: "/w", title }),
    ),
    providers: vi.fn(async () => ({ providers: PROVIDERS, default: "deepseek" })),
    pickFolder: vi.fn(async () => options.picked ?? null),
    openWorkspace: vi.fn(async (path: string) => ({ workspace: path, restricted: true })),
    onSidecarState: vi.fn((listener: (status: SidecarStatus) => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    }),
    pushState: (status: SidecarStatus) => {
      for (const listener of listeners) listener(status);
    },
  };
  return client as unknown as FakeClient;
}
