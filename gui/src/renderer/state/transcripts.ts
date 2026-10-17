import { useCallback, useEffect, useReducer, useRef } from "react";
import type { EventFrame } from "../../shared/protocol";
import type { Answer } from "../lib/client";
import { useClient } from "./client-context";
import {
  type CoreEvent,
  clearPending,
  dropAction,
  interruptTurn,
  optimisticTurn,
  type PendingAction,
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
  | {
      type: "loaded";
      sessionId: string;
      turns: TurnView[];
      /** The workspace's running turn when it belongs to this session. */
      active: { workspace: string } | null;
    }
  | {
      type: "attached";
      workspace: string;
      sessionId: string;
      frames: EventFrame[];
      activeTurnId: string | null;
    }
  | { type: "interrupt_all" }
  | { type: "load_failed"; sessionId: string }
  | { type: "sent"; workspace: string; sessionId: string; input: string }
  | { type: "send_failed"; workspace: string; sessionId: string }
  | { type: "stopping"; workspace: string }
  | { type: "dismiss"; sessionId: string; actionId: string }
  | { type: "reconcile"; listed: ReadonlySet<string>; asked: ReadonlySet<string> }
  | { type: "dismiss"; sessionId: string; actionId: string }
  | { type: "reconcile"; listed: ReadonlySet<string>; asked: ReadonlySet<string> }
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
        active: action.active
          ? {
              ...state.active,
              [action.active.workspace]: {
                sessionId: action.sessionId,
                stopping: state.active[action.active.workspace]?.stopping ?? false,
              },
            }
          : state.active,
      };
    }
    case "attached": {
      // A session that is not on screen only learns that its workspace has a turn running.
      const loaded = state.turns[action.sessionId] !== undefined;
      const applied = loaded ? action.frames.reduce(applyFrame, state) : state;
      return {
        ...applied,
        active: action.activeTurnId
          ? {
              ...applied.active,
              [action.workspace]: { sessionId: action.sessionId, stopping: false },
            }
          : applied.active,
      };
    }
    case "interrupt_all":
      // The process is gone: no `turn.done` will come, and nothing is still waiting.
      return {
        ...state,
        turns: Object.fromEntries(
          Object.entries(state.turns).map(([id, list]) => [
            id,
            list.map((t) => (t.status === "running" ? interruptTurn(t) : t)),
          ]),
        ),
        active: {},
      };
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
      // Stopping resolves every waiting action as denied, so their dialogs close at once.
      const turns = state.turns[current.sessionId] ?? [];
      return {
        ...state,
        turns: { ...state.turns, [current.sessionId]: turns.map(clearPending) },
        active: { ...state.active, [action.workspace]: { ...current, stopping: true } },
      };
    }
    case "dismiss":
      return {
        ...state,
        turns: {
          ...state.turns,
          [action.sessionId]: (state.turns[action.sessionId] ?? []).map((t) =>
            dropAction(t, action.actionId),
          ),
        },
      };
    case "reconcile": {
      // Only actions that were already waiting when the list was asked for can be judged gone.
      const turns = Object.fromEntries(
        Object.entries(state.turns).map(([id, list]) => [
          id,
          list.map((t) => ({
            ...t,
            pending: t.pending.filter(
              (a) => !action.asked.has(a.actionId) || action.listed.has(a.actionId),
            ),
          })),
        ]),
      );
      return { ...state, turns };
    }
    case "frames":
      return action.frames.reduce(applyFrame, state);
  }
}

/** What the screen's other parts need to know about each frame. */
export interface FrameInfo {
  type: string;
  workspace: string;
  sessionId: string;
  /** A response from a tool that can change files. */
  mutating: boolean;
}

export interface Transcripts {
  state: TranscriptState;
  /** Loads a session's stored turns (the workspace must be open). */
  load(workspace: string, sessionId: string): Promise<void>;
  /** Shows the prompt at once, then asks the sidecar; rejects, with the prompt removed, on failure. */
  send(workspace: string, sessionId: string, input: string): Promise<void>;
  stop(workspace: string, sessionId: string): Promise<void>;
  /** Applies an event that did not arrive as a frame, such as the `turn.undone` an undo returns. */
  applyEvent(workspace: string, sessionId: string, turnId: string | null, event: CoreEvent): void;
  /** Answers a required action; it leaves the screen at once, and a refusal from the sidecar changes nothing. */
  reply(workspace: string, sessionId: string, actionId: string, answer: Answer): Promise<void>;
  /** Drops the pending actions the sidecar no longer lists. */
  reconcile(): Promise<void>;
  /** Attaches to every workspace's running turn, as the window starts or the sidecar returns. */
  catchUp(): Promise<void>;
  /** The sidecar died: every running turn is interrupted and nothing is pending. */
  interruptAll(): void;
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
export function useTranscripts(onFrame: (info: FrameInfo) => void): Transcripts {
  const client = useClient();
  const [state, dispatch] = useReducer(transcriptReducer, emptyTranscripts);
  const queue = useRef<EventFrame[]>([]);
  const scheduled = useRef(false);
  const edge = useRef(onFrame);
  edge.current = onFrame;
  // `tool.response` carries no tool name, so the calls are remembered by id.
  const toolNames = useRef(new Map<string, string>());

  useEffect(() => {
    const flush = () => {
      scheduled.current = false;
      const frames = queue.current;
      queue.current = [];
      if (frames.length > 0) dispatch({ type: "frames", frames });
    };
    return client.onSidecarEvent((frame) => {
      queue.current.push(frame);
      const event = frame.event as CoreEvent;
      if (event.type === "tool.call")
        toolNames.current.set(String(event.tool_call_id), String(event.name));
      const name =
        event.type === "tool.response"
          ? toolNames.current.get(String(event.tool_call_id))
          : undefined;
      edge.current({
        type: event.type,
        workspace: frame.workspace,
        sessionId: frame.session_id,
        mutating: name === "edit" || name === "write_file" || name === "bash",
      });
      if (!scheduled.current) {
        scheduled.current = true;
        schedule(flush);
      }
    });
  }, [client]);

  const applyEvent = useCallback(
    (workspace: string, sessionId: string, turnId: string | null, event: CoreEvent) =>
      dispatch({
        type: "frames",
        frames: [
          { kind: "event", workspace, session_id: sessionId, turn_id: turnId, event } as EventFrame,
        ],
      }),
    [],
  );

  const reconcileRef = useRef<() => Promise<void>>(async () => undefined);

  const load = useCallback(
    async (workspace: string, sessionId: string) => {
      dispatch({ type: "loading", sessionId });
      try {
        const { turns } = await client.resume(workspace, sessionId);
        // Attached after the resume, so a turn that ended in between still arrives with its end.
        const live = await client
          .attachLive(workspace, sessionId)
          .catch(() => ({ frames: [], activeTurnId: null }));
        dispatch({
          type: "loaded",
          sessionId,
          turns: turns.map((t) => turnFromRecord(t as never, live.activeTurnId)),
          active: live.activeTurnId ? { workspace } : null,
        });
        // Frames the stored turns already hold are dropped by their event ids.
        if (live.frames.length > 0) dispatch({ type: "frames", frames: live.frames });
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

  const reply = useCallback(
    async (workspace: string, sessionId: string, actionId: string, answer: Answer) => {
      dispatch({ type: "dismiss", sessionId, actionId });
      // `unknown_action` means it was already gone; either way the dialog is closed.
      await client.replyAction(workspace, sessionId, actionId, answer).catch(() => undefined);
    },
    [client],
  );

  const stateRef = useRef(state);
  stateRef.current = state;
  const reconcile = useCallback(async () => {
    const asked = new Set(
      Object.values(stateRef.current.turns).flatMap((list) =>
        list.flatMap((t) => t.pending.map((a) => a.actionId)),
      ),
    );
    if (asked.size === 0) return;
    const listed = new Set(await client.pendingActionIds().catch(() => [...asked]));
    dispatch({ type: "reconcile", listed, asked });
  }, [client]);

  reconcileRef.current = reconcile;

  const catchUp = useCallback(async () => {
    const running = await client.activeTurns().catch(() => []);
    for (const { workspace, sessionId } of running) {
      const live = await client.attachLive(workspace, sessionId).catch(() => null);
      if (live) dispatch({ type: "attached", workspace, sessionId, ...live });
    }
  }, [client]);

  const interruptAll = useCallback(() => dispatch({ type: "interrupt_all" }), []);

  const markFailed = useCallback(
    (sessionId: string) => dispatch({ type: "load_failed", sessionId }),
    [],
  );

  return {
    state,
    load,
    send,
    stop,
    reply,
    reconcile,
    applyEvent,
    markFailed,
    catchUp,
    interruptAll,
  };
}

/** Every pending action across a session's turns, oldest first. */
export function pendingOf(state: TranscriptState, sessionId: string): PendingAction[] {
  return (state.turns[sessionId] ?? []).flatMap((t) => t.pending);
}
