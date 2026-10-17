import { useCallback, useEffect, useRef, useState } from "react";
import type { Session } from "../../shared/protocol";
import { useClient } from "./client-context";

const RETRY_MS = 1000;
const MAX_ATTEMPTS = 20;

export interface SessionsData {
  /** `loading` until the sidecar answers the first list. */
  status: "loading" | "ready";
  sessions: Session[];
  /** A turn is running in the session now. */
  running: ReadonlySet<string>;
  /** The sidecar stopped while a turn ran in the session. */
  interrupted: ReadonlySet<string>;
  rename(session: Session, title: string): Promise<void>;
}

/** The sessions from the sidecar, with the running and interrupted marks derived from its state. */
export function useSessions(): SessionsData {
  const client = useClient();
  const [status, setStatus] = useState<SessionsData["status"]>("loading");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [running, setRunning] = useState<ReadonlySet<string>>(new Set());
  const [interrupted, setInterrupted] = useState<ReadonlySet<string>>(new Set());
  const runningNow = useRef<ReadonlySet<string>>(new Set());
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const load = useCallback(
    async (attempt: number) => {
      clearTimeout(timer.current);
      try {
        const [list, active] = await Promise.all([client.listSessions(), client.runningSessions()]);
        const ids = new Set(active);
        runningNow.current = ids;
        setSessions(list);
        setRunning(ids);
        setStatus("ready");
      } catch {
        // The sidecar is still starting, or restarting: ask again shortly.
        if (attempt < MAX_ATTEMPTS)
          timer.current = setTimeout(() => void load(attempt + 1), RETRY_MS);
      }
    },
    [client],
  );

  useEffect(() => {
    void load(1);
    const stop = client.onSidecarState(({ state }) => {
      if (state === "ready") void load(1);
      if (state === "crashed") {
        // Whatever was running died with the process; the transcript will say so on resume.
        setInterrupted((before) => new Set([...before, ...runningNow.current]));
        runningNow.current = new Set();
        setRunning(new Set());
      }
    });
    return () => {
      stop();
      clearTimeout(timer.current);
    };
  }, [client, load]);

  const rename = useCallback(
    async (session: Session, title: string) => {
      const updated = await client.renameSession(session.workspace, session.id, title);
      setSessions((list) => list.map((s) => (s.id === updated.id ? updated : s)));
    },
    [client],
  );

  return { status, sessions, running, interrupted, rename };
}

/** Sessions grouped by workspace: groups by their newest session, sessions newest first. */
export function groupByWorkspace(
  sessions: Session[],
): Array<{ workspace: string; sessions: Session[] }> {
  const groups = new Map<string, Session[]>();
  for (const session of sessions) {
    groups.set(session.workspace, [...(groups.get(session.workspace) ?? []), session]);
  }
  const newest = (list: Session[]) =>
    list
      .map((s) => s.updated_at)
      .sort()
      .at(-1) ?? "";
  return [...groups.entries()]
    .map(([workspace, list]) => ({
      workspace,
      sessions: [...list].sort((a, b) => b.updated_at.localeCompare(a.updated_at)),
    }))
    .sort((a, b) => newest(b.sessions).localeCompare(newest(a.sessions)));
}
