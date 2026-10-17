import { useCallback, useEffect, useRef, useState } from "react";
import type { Result } from "../../shared/protocol";
import { useClient } from "./client-context";
import type { ChangesScope } from "./store";

export type FileChange = Result<"changes.workspace">["files"][number];

export interface ChangesEntry {
  status: "idle" | "loading" | "ready" | "error";
  files: FileChange[];
  /** From the workspace scope: whether the folder is a git repository. `null` until known. */
  git: boolean | null;
  /** From the turn scope: whether the last turn has a checkpoint. `null` until known. */
  checkpoint: boolean | null;
  error?: string;
}

export const DEBOUNCE_MS = 400;

const empty: ChangesEntry = { status: "idle", files: [], git: null, checkpoint: null };

export interface Changes {
  turn: ChangesEntry;
  workspace: ChangesEntry;
  /** Asks again: now, or after the 400 ms trailing debounce. Several asks make one request. */
  refresh(scope: ChangesScope | "both", options?: { immediate?: boolean }): void;
  /** Resolves when the scope's current request (or the one waiting behind it) is done. */
  settled(scope: ChangesScope): Promise<void>;
}

/**
 * The two scopes of changes for one session. Each scope has one request in flight at a time; asks
 * that arrive meanwhile become exactly one follow-up. The previous list stays while it loads.
 */
export function useChanges(target: { workspace: string; sessionId: string } | null): Changes {
  const client = useClient();
  const [entries, setEntries] = useState<Record<ChangesScope, ChangesEntry>>({
    turn: empty,
    workspace: empty,
  });
  const key = target ? `${target.workspace}|${target.sessionId}` : null;
  const targetRef = useRef(target);
  targetRef.current = target;

  const flight = useRef<
    Record<ChangesScope, { running: boolean; again: boolean; waiters: Array<() => void> }>
  >({
    turn: { running: false, again: false, waiters: [] },
    workspace: { running: false, again: false, waiters: [] },
  });
  const timers = useRef<Partial<Record<ChangesScope, ReturnType<typeof setTimeout>>>>({});

  const run = useCallback(
    async (scope: ChangesScope) => {
      const state = flight.current[scope];
      if (state.running) {
        state.again = true;
        return;
      }
      state.running = true;
      do {
        state.again = false;
        const asked = targetRef.current;
        if (!asked) break;
        setEntries((all) => ({ ...all, [scope]: { ...all[scope], status: "loading" } }));
        try {
          const result =
            scope === "turn"
              ? await client.changesTurn(asked.workspace, asked.sessionId)
              : await client.changesWorkspace(asked.workspace);
          const current = targetRef.current;
          // An answer for a session that is no longer on screen is dropped.
          if (current?.workspace === asked.workspace && current.sessionId === asked.sessionId) {
            setEntries((all) => ({
              ...all,
              [scope]: {
                status: "ready",
                files: result.files,
                git: "git" in result ? result.git : all[scope].git,
                checkpoint: "checkpoint" in result ? result.checkpoint : all[scope].checkpoint,
              },
            }));
          }
        } catch (error) {
          const message = error instanceof Error ? error.message : "The request failed.";
          setEntries((all) => ({
            ...all,
            [scope]: { ...all[scope], status: "error", error: message },
          }));
        }
      } while (state.again);
      state.running = false;
      for (const done of state.waiters.splice(0)) done();
    },
    [client],
  );

  const refresh = useCallback(
    (scope: ChangesScope | "both", options: { immediate?: boolean } = {}) => {
      if (!targetRef.current) return;
      for (const one of scope === "both" ? (["turn", "workspace"] as const) : [scope]) {
        clearTimeout(timers.current[one]);
        if (options.immediate) {
          delete timers.current[one];
          void run(one);
        } else {
          timers.current[one] = setTimeout(() => {
            delete timers.current[one];
            void run(one);
          }, DEBOUNCE_MS);
        }
      }
    },
    [run],
  );

  const settled = useCallback((scope: ChangesScope) => {
    const state = flight.current[scope];
    if (!state.running && timers.current[scope] === undefined) return Promise.resolve();
    return new Promise<void>((resolve) => state.waiters.push(resolve));
  }, []);

  // Another session or workspace is another list: start empty, and drop any waiting ask.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` is the trigger
  useEffect(() => {
    setEntries({ turn: empty, workspace: empty });
    for (const timer of Object.values(timers.current)) clearTimeout(timer);
    timers.current = {};
    for (const state of Object.values(flight.current))
      for (const done of state.waiters.splice(0)) done();
  }, [key]);
  useEffect(
    () => () => {
      for (const timer of Object.values(timers.current)) clearTimeout(timer);
    },
    [],
  );

  return { turn: entries.turn, workspace: entries.workspace, refresh, settled };
}
