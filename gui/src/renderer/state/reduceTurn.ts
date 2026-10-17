// The pure turn reducer. Live frames and replayed events go through the same function; it reads
// no clock (times come from event id timestamps), and the final `model.message` is
// authoritative over any streamed deltas.

import { type ParsedDiff, parseDiff } from "./diff";

export interface CoreEvent {
  id: string;
  type: string;
  [field: string]: unknown;
}

export type TurnStatus = "running" | "done" | "error" | "cancelled" | "interrupted";
export type ToolState = "running" | "awaiting_approval" | "done" | "blocked" | "interrupted";
export type ThreadStatus = "running" | "done" | "error" | "interrupted";

export interface AssistantItem {
  kind: "assistant";
  threadId: string;
  messageId: string;
  text: string;
  final: boolean;
}

export interface ToolItem {
  kind: "tool";
  threadId: string;
  toolCallId: string;
  name: string;
  /** The arguments exactly as the model sent them. */
  rawArgs: string;
  /** `null` when the text is not a JSON object: the card then shows the raw text. */
  args: Record<string, unknown> | null;
  state: ToolState;
  output?: string;
  /** The guardrail rule, when the call was blocked. */
  rule?: string;
  exitCode?: number;
  diff?: ParsedDiff;
  /** For `create_sub_agent`: the subagent threads that belong to this card. */
  children: string[];
}

/** A required action waiting for the user: an approval for one tool call, or a question. */
export type PendingAction =
  | {
      kind: "approval";
      actionId: string;
      threadId: string;
      toolCallId: string;
      name: string;
      rawArgs: string;
      rule: string;
    }
  | { kind: "question"; actionId: string; threadId: string; question: string; options: string[] };

export interface NoticeItem {
  kind: "notice";
  id: string;
  text: string;
}

export type Item = AssistantItem | ToolItem | NoticeItem;

export interface ThreadView {
  title: string;
  status: ThreadStatus;
  error?: string;
  items: Item[];
}

export interface TurnView {
  /** `null` only for the optimistic turn, until `turn.created` arrives. */
  turnId: string | null;
  input: string;
  status: TurnStatus;
  error: string | null;
  /** The root thread's items, in event order. */
  items: Item[];
  /** Subagent threads by id; they render inside their `create_sub_agent` card, never in `items`. */
  threads: Record<string, ThreadView>;
  /** Oldest first. Gone when answered, when the turn ends, or when the sidecar no longer lists them. */
  pending: PendingAction[];
  /** The workspace was restored to before this turn (a `turn.undone` event, replayed or live). */
  undone: boolean;
  startedMs: number | null;
  finishedMs: number | null;
  /** Event ids already applied: applying one twice changes nothing. */
  seen: ReadonlySet<string>;
}

export function optimisticTurn(input: string): TurnView {
  return {
    turnId: null,
    input,
    status: "running",
    error: null,
    items: [],
    threads: {},
    pending: [],
    undone: false,
    startedMs: null,
    finishedMs: null,
    seen: new Set(),
  };
}

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** The time a ULID was made, in milliseconds; `null` for anything that is not a ULID. */
export function ulidMs(id: string): number | null {
  if (!/^[0-9a-hjkmnp-tv-zA-HJKMNP-TV-Z]{26}$/.test(id)) return null;
  let ms = 0;
  for (const char of id.slice(0, 10).toUpperCase()) ms = ms * 32 + CROCKFORD.indexOf(char);
  return ms;
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function parseArgs(raw: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(raw);
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

/** Applies `change` to the items of a thread: a subagent's own list, or the root's. */
function inThread(turn: TurnView, threadId: string, change: (items: Item[]) => Item[]): TurnView {
  const thread = turn.threads[threadId];
  if (!thread) return { ...turn, items: change(turn.items) };
  return {
    ...turn,
    threads: { ...turn.threads, [threadId]: { ...thread, items: change(thread.items) } },
  };
}

/** Every item of the turn with the list that holds it, roots first. */
function allItems(turn: TurnView): Item[] {
  return [...turn.items, ...Object.values(turn.threads).flatMap((t) => t.items)];
}

function mapTool(
  turn: TurnView,
  toolCallId: string,
  change: (tool: ToolItem) => ToolItem,
): TurnView {
  const swap = (items: Item[]) =>
    items.map((item) =>
      item.kind === "tool" && item.toolCallId === toolCallId ? change(item) : item,
    );
  return {
    ...turn,
    items: swap(turn.items),
    threads: Object.fromEntries(
      Object.entries(turn.threads).map(([id, t]) => [id, { ...t, items: swap(t.items) }]),
    ),
  };
}

function upsert(items: Item[], next: AssistantItem): Item[] {
  const index = items.findIndex(
    (item) =>
      item.kind === "assistant" &&
      item.threadId === next.threadId &&
      item.messageId === next.messageId,
  );
  if (index < 0) return [...items, next];
  return items.map((item, i) => (i === index ? next : item));
}

function findAssistant(
  items: Item[],
  threadId: string,
  messageId: string,
): AssistantItem | undefined {
  return items.find(
    (i): i is AssistantItem =>
      i.kind === "assistant" && i.threadId === threadId && i.messageId === messageId,
  );
}

function itemsOf(turn: TurnView, threadId: string): Item[] {
  return turn.threads[threadId]?.items ?? turn.items;
}

/**
 * The `create_sub_agent` card a new thread belongs to: the oldest unclaimed one in the parent
 * thread whose `name` equals the thread's title, else the oldest unclaimed one.
 */
function subagentCard(turn: TurnView, parent: string, title: string): ToolItem | undefined {
  const open = itemsOf(turn, parent).filter(
    (i): i is ToolItem =>
      i.kind === "tool" &&
      i.name === "create_sub_agent" &&
      i.state === "running" &&
      i.children.length === 0,
  );
  return open.find((t) => t.args?.name === title) ?? open[0];
}

/** One event applied to a turn. Unknown types are remembered as seen and otherwise ignored. */
export function reduceTurn(turn: TurnView, event: CoreEvent): TurnView {
  if (turn.seen.has(event.id)) return turn;
  const seen = new Set(turn.seen).add(event.id);
  const base: TurnView = { ...turn, seen };
  const threadId = str(event.thread_id);
  const messageId = str(event.message_id);

  switch (event.type) {
    case "turn.created":
      return {
        ...base,
        turnId: str(event.turn_id) || turn.turnId,
        status: "running",
        startedMs: ulidMs(event.id),
      };

    case "turn.undone":
      return { ...base, undone: true };

    case "model.message": {
      const message = event.message as { content?: unknown } | null | undefined;
      const existing = findAssistant(itemsOf(turn, threadId), threadId, messageId);
      if (!message) {
        // The live start marker; a message that is already final keeps its text.
        if (existing) return base;
        return inThread(base, threadId, (items) =>
          upsert(items, { kind: "assistant", threadId, messageId, text: "", final: false }),
        );
      }
      const text = str(message.content);
      // A tool-only round has an empty final message: it renders nothing, so it is removed.
      if (text === "") {
        return inThread(base, threadId, (items) =>
          items.filter(
            (i) =>
              !(i.kind === "assistant" && i.threadId === threadId && i.messageId === messageId),
          ),
        );
      }
      return inThread(base, threadId, (items) =>
        upsert(items, { kind: "assistant", threadId, messageId, text, final: true }),
      );
    }

    case "model.message.delta": {
      const existing = findAssistant(itemsOf(turn, threadId), threadId, messageId);
      // The final text already holds everything the deltas would add.
      if (existing?.final) return base;
      const text = (existing?.text ?? "") + str(event.content);
      return inThread(base, threadId, (items) =>
        upsert(items, { kind: "assistant", threadId, messageId, text, final: false }),
      );
    }

    case "tool.call": {
      const rawArgs = str(event.arguments);
      const toolCallId = str(event.tool_call_id);
      const tool: ToolItem = {
        kind: "tool",
        threadId,
        toolCallId,
        name: str(event.name),
        rawArgs,
        args: parseArgs(rawArgs),
        state: "running",
        children: [],
      };
      return inThread(base, threadId, (items) => [...items, tool]);
    }

    case "tool.approval_required": {
      const toolCallId = str(event.tool_call_id);
      const action: PendingAction = {
        kind: "approval",
        actionId: str(event.action_id),
        threadId,
        toolCallId,
        name: str(event.name),
        rawArgs: str(event.arguments),
        rule: str(event.rule),
      };
      const marked = mapTool(base, toolCallId, (tool) =>
        tool.state === "running"
          ? { ...tool, state: "awaiting_approval", rule: action.rule }
          : tool,
      );
      return { ...marked, pending: [...marked.pending, action] };
    }

    case "tool.response_required": {
      const action: PendingAction = {
        kind: "question",
        actionId: str(event.action_id),
        threadId,
        question: str(event.question),
        options: Array.isArray(event.options) ? event.options.map(String) : [],
      };
      return { ...base, pending: [...base.pending, action] };
    }

    case "tool.response": {
      const content = str(event.content);
      const blocked = event.blocked === true;
      const diff = typeof event.diff === "string" ? parseDiff(event.diff) : undefined;
      const callId = str(event.tool_call_id);
      const answered = allItems(base).find(
        (i): i is ToolItem => i.kind === "tool" && i.toolCallId === callId,
      );
      // The call is over, so any approval for it is too; an answered question leaves its thread.
      const open: TurnView = {
        ...base,
        pending: base.pending.filter(
          (a) =>
            !(a.kind === "approval" && a.toolCallId === callId) &&
            !(
              a.kind === "question" &&
              answered?.name === "ask_question" &&
              a.threadId === answered.threadId
            ),
        ),
      };
      return mapTool(open, callId, (tool) => {
        if (blocked) {
          return {
            ...tool,
            state: "blocked",
            rule: content.replace(/^Blocked by guardrail: ?/, ""),
            output: content,
          };
        }
        // `bash` output starts with `exit code: N`.
        const exit = tool.name === "bash" ? /^exit code: (\d+)\n?/.exec(content) : null;
        return {
          ...tool,
          state: "done",
          output: exit ? content.slice(exit[0].length) : content,
          ...(exit ? { exitCode: Number(exit[1]) } : {}),
          ...(diff ? { diff } : {}),
        };
      });
    }

    case "context.compacted": {
      const usage = event.usage as { prompt_tokens?: number } | undefined;
      const size =
        typeof usage?.prompt_tokens === "number" ? ` · ${usage.prompt_tokens} tokens` : "";
      return inThread(base, threadId, (items) => [
        ...items,
        { kind: "notice", id: event.id, text: `Context compacted${size}` },
      ]);
    }

    case "thread.created": {
      const title = str(event.title);
      const card = subagentCard(base, str(event.parent), title);
      const withThread: TurnView = {
        ...base,
        threads: { ...base.threads, [threadId]: { title, status: "running", items: [] } },
      };
      return card
        ? mapTool(withThread, card.toolCallId, (tool) => ({
            ...tool,
            children: [...tool.children, threadId],
          }))
        : withThread;
    }

    case "thread.done": {
      const thread = base.threads[threadId];
      if (!thread) return base;
      const error = typeof event.error === "string" ? event.error : undefined;
      return {
        ...base,
        threads: {
          ...base.threads,
          [threadId]: { ...thread, status: error ? "error" : "done", ...(error ? { error } : {}) },
        },
      };
    }

    case "turn.done": {
      const state = str(event.state);
      const status: TurnStatus = state === "error" || state === "cancelled" ? state : "done";
      return {
        ...base,
        status,
        pending: [],
        error: typeof event.error === "string" ? event.error : null,
        finishedMs: ulidMs(event.id),
      };
    }

    default:
      return base;
  }
}

/** A subagent group's status: an error in any child wins, then running, else done. */
export function groupStatus(turn: TurnView, tool: ToolItem): ThreadStatus {
  const children = tool.children.map((id) => turn.threads[id]?.status);
  if (children.includes("error")) return "error";
  if (children.includes("interrupted")) return "interrupted";
  if (children.includes("running") || (children.length === 0 && tool.state === "running"))
    return "running";
  return "done";
}

/** What the turn changed through `edit` and `write_file`: one file once, counts summed. */
export interface TurnSummary {
  files: Array<{ path: string; added: number; removed: number }>;
  added: number;
  removed: number;
  durationMs: number | null;
}

export function summarize(turn: TurnView): TurnSummary {
  const byPath = new Map<string, { path: string; added: number; removed: number }>();
  for (const item of allItems(turn)) {
    if (item.kind !== "tool" || item.state !== "done" || !item.diff) continue;
    if (item.name !== "edit" && item.name !== "write_file") continue;
    const path = str(item.args?.path) || "(unknown file)";
    const entry = byPath.get(path) ?? { path, added: 0, removed: 0 };
    entry.added += item.diff.added;
    entry.removed += item.diff.removed;
    byPath.set(path, entry);
  }
  const files = [...byPath.values()];
  const duration =
    turn.startedMs !== null && turn.finishedMs !== null ? turn.finishedMs - turn.startedMs : null;
  return {
    files,
    added: files.reduce((n, f) => n + f.added, 0),
    removed: files.reduce((n, f) => n + f.removed, 0),
    durationMs: duration,
  };
}

function interruptItems(items: Item[]): Item[] {
  return items.map((i) =>
    i.kind === "tool" && (i.state === "running" || i.state === "awaiting_approval")
      ? { ...i, state: "interrupted" }
      : i,
  );
}

/** The turn without the action (answered, or no longer waiting). */
export function dropAction(turn: TurnView, actionId: string): TurnView {
  const pending = turn.pending.filter((a) => a.actionId !== actionId);
  return pending.length === turn.pending.length ? turn : { ...turn, pending };
}

/** The turn without its pending actions, e.g. when Stop was pressed. */
export function clearPending(turn: TurnView): TurnView {
  return turn.pending.length === 0 ? turn : { ...turn, pending: [] };
}

/** A stored turn (`TurnRecord`) rebuilt as a view. A turn that never finished is interrupted. */
export function turnFromRecord(record: {
  turn_id: string;
  input: string;
  state: string | null;
  error: string | null;
  events: CoreEvent[];
}): TurnView {
  let turn: TurnView = { ...optimisticTurn(record.input), turnId: record.turn_id };
  // Stored events are in write order, which is the order the live stream delivered them.
  for (const event of record.events) turn = reduceTurn(turn, event);
  if (record.state === null && turn.status === "running") {
    turn = {
      ...turn,
      status: "interrupted",
      // The action went with the process that was waiting on it.
      pending: [],
      items: interruptItems(turn.items),
      threads: Object.fromEntries(
        Object.entries(turn.threads).map(([id, t]) => [
          id,
          {
            ...t,
            status: t.status === "running" ? "interrupted" : t.status,
            items: interruptItems(t.items),
          },
        ]),
      ),
    };
  }
  return turn;
}
