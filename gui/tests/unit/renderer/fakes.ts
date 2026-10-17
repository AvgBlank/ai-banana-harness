import { type Mocked, vi } from "vitest";
import type { Client, SidecarStatus } from "../../../src/renderer/lib/client";
import type { Prefs } from "../../../src/shared/api";
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

export const PROVIDERS: ProviderOption[] = [
  { provider: "deepseek", model: "deepseek-flash", host: "api.deepseek.com", scheme: "https" },
  { provider: "qwen", model: "qwen3.8-flash", host: "dashscope.aliyuncs.com", scheme: "https" },
];

export type FakeClient = Mocked<Client> & { pushState(status: SidecarStatus): void };

/** An in-memory stand-in for the preload API; every method is a spy. */
export function fakeClient(
  options: { sessions?: Session[]; running?: string[]; prefs?: Prefs; picked?: string | null } = {},
): FakeClient {
  const listeners = new Set<(status: SidecarStatus) => void>();
  const stored: Prefs = options.prefs ?? { theme: "dark" };
  const client = {
    appInfo: vi.fn(async () => ({
      name: "Test App",
      version: "0.1.0",
      platform: "darwin",
      protocol: 1,
    })),
    prefs: vi.fn(async () => stored),
    setPrefs: vi.fn(async (patch: Partial<Prefs>) => Object.assign(stored, patch)),
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
