import { useCallback, useEffect, useReducer, useRef } from "react";
import type { EventFrame } from "../../shared/protocol";
import { useClient } from "./client-context";
import {
  type CoreEvent,
  optimisticTurn,
  reduceTurn,
  type TurnView,
  turnFromRecord,
} from "./reduceTurn";

export type LoadStatus = "loading" | "ready" | "error";

export interface TranscriptState {
  turns: Record<string, TurnView[]>;
  load: Record<string, LoadStatus>;
  /** The workspace's running turn, if any; `stopping` from Stop until `turn.done`. */
  active: Record<string, { sessionId: string; stopping: boolean }>;
}

type Action =
  | { type: "loading"; sessionId: string }
  | { type: "loaded"; sessionId: string; turns: TurnView[] }
  | { type: "load_failed"; sessionId: string }
  | { type: "sent"; workspace: string; sessionId: string; input: string }
  | { type: "send_failed"; workspace: string; sessionId: string }
  | { type: "stopping"; workspace: string }
  | { type: "frames"; frames: EventFrame[] };

export const emptyTranscripts: TranscriptState = { turns: {}, load: {}, active: {} };

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const { [key]: _removed, ...rest } = record;
  return rest;
}

function applyFrame(state: TranscriptState, frame: EventFrame): TranscriptState {
  const sessionId = frame.session_id;
  const turns = state.turns[sessionId] ?? [];
  let index = frame.turn_id === null ? -1 : turns.findIndex((t) => t.turnId === frame.turn_id);
  // An event for a turn not yet bound belongs to the optimistic turn.
  if (index < 0) index = turns.findLastIndex((t) => t.turnId === null);
  const base = index >= 0 ? (turns[index] as TurnView) : optimisticTurn("");
  const next = reduceTurn(base, frame.event as CoreEvent);
  const updated = index >= 0 ? turns.map((t, i) => (i === index ? next : t)) : [...turns, next];
  let active = state.active;
  if (frame.event.type === "turn.done" && active[frame.workspace]?.sessionId === sessionId) {
    active = without(active, frame.workspace);
  }
  return { ...state, turns: { ...state.turns, [sessionId]: updated }, active };
}

export function transcriptReducer(state: TranscriptState, action: Action): TranscriptState {
  switch (action.type) {
    case "loading":
      return { ...state, load: { ...state.load, [action.sessionId]: "loading" } };
    case "loaded": {
      // Live turns that the stored copy does not have yet (just started) are kept.
      const stored = new Set(action.turns.map((t) => t.turnId));
      const live = (state.turns[action.sessionId] ?? []).filter(
        (t) => t.turnId === null || !stored.has(t.turnId),
      );
      const keep = live.filter((t) => t.turnId === null || t.status === "running");
      return {
        ...state,
        turns: { ...state.turns, [action.sessionId]: [...action.turns, ...keep] },
        load: { ...state.load, [action.sessionId]: "ready" },
      };
    }
    case "load_failed":
      return { ...state, load: { ...state.load, [action.sessionId]: "error" } };
    case "sent":
      return {
        ...state,
        turns: {
          ...state.turns,
          [action.sessionId]: [
            ...(state.turns[action.sessionId] ?? []),
            optimisticTurn(action.input),
          ],
        },
        load: { ...state.load, [action.sessionId]: state.load[action.sessionId] ?? "ready" },
        active: {
          ...state.active,
          [action.workspace]: { sessionId: action.sessionId, stopping: false },
        },
      };
    case "send_failed":
      return {
        ...state,
        turns: {
          ...state.turns,
          [action.sessionId]: (state.turns[action.sessionId] ?? []).filter(
            (t) => t.turnId !== null,
          ),
        },
        active: without(state.active, action.workspace),
      };
    case "stopping": {
      const current = state.active[action.workspace];
      if (!current) return state;
      return {
        ...state,
        active: { ...state.active, [action.workspace]: { ...current, stopping: true } },
      };
    }
    case "frames":
      return action.frames.reduce(applyFrame, state);
  }
}

export interface Transcripts {
  state: TranscriptState;
  /** Loads a session's stored turns (the workspace must be open). */
  load(workspace: string, sessionId: string): Promise<void>;
  /** Shows the prompt at once, then asks the sidecar; rejects, with the prompt removed, on failure. */
  send(workspace: string, sessionId: string, input: string): Promise<void>;
  stop(workspace: string, sessionId: string): Promise<void>;
  /** The session could not be opened, so its turns cannot be read. */
  markFailed(sessionId: string): void;
}

const schedule = (run: () => void): void => {
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(run);
  else setTimeout(run, 16);
};

/**
 * The transcripts of the sessions the user has looked at. Streamed frames are collected and
 * applied once per animation frame, so a fast stream costs one render per frame.
 */
export function useTranscripts(onTurnEdge: () => void): Transcripts {
  const client = useClient();
  const [state, dispatch] = useReducer(transcriptReducer, emptyTranscripts);
  const queue = useRef<EventFrame[]>([]);
  const scheduled = useRef(false);
  const edge = useRef(onTurnEdge);
  edge.current = onTurnEdge;

  useEffect(() => {
    const flush = () => {
      scheduled.current = false;
      const frames = queue.current;
      queue.current = [];
      if (frames.length > 0) dispatch({ type: "frames", frames });
    };
    return client.onSidecarEvent((frame) => {
      queue.current.push(frame);
      // The sidebar's auto-title and badges follow a turn's start and end.
      if (frame.event.type === "turn.created" || frame.event.type === "turn.done") edge.current();
      if (!scheduled.current) {
        scheduled.current = true;
        schedule(flush);
      }
    });
  }, [client]);

  const load = useCallback(
    async (workspace: string, sessionId: string) => {
      dispatch({ type: "loading", sessionId });
      try {
        const { turns } = await client.resume(workspace, sessionId);
        dispatch({
          type: "loaded",
          sessionId,
          turns: turns.map((t) => turnFromRecord(t as never)),
        });
      } catch {
        dispatch({ type: "load_failed", sessionId });
      }
    },
    [client],
  );

  const send = useCallback(
    async (workspace: string, sessionId: string, input: string) => {
      dispatch({ type: "sent", workspace, sessionId, input });
      try {
        await client.startTurn(workspace, sessionId, input);
      } catch (error) {
        dispatch({ type: "send_failed", workspace, sessionId });
        throw error;
      }
    },
    [client],
  );

  const stop = useCallback(
    async (workspace: string, sessionId: string) => {
      dispatch({ type: "stopping", workspace });
      await client.cancelTurn(workspace, sessionId).catch(() => undefined);
    },
    [client],
  );

  const markFailed = useCallback(
    (sessionId: string) => dispatch({ type: "load_failed", sessionId }),
    [],
  );

  return { state, load, send, stop, markFailed };
}
