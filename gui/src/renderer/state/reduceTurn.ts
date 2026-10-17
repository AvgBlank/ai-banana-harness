// The pure turn reducer. Live frames and replayed events go through the same function; it reads
// no clock, and the final `model.message` is authoritative over any streamed deltas.

export interface CoreEvent {
  id: string;
  type: string;
  [field: string]: unknown;
}

export type TurnStatus = "running" | "done" | "error" | "cancelled" | "interrupted";

export interface AssistantItem {
  kind: "assistant";
  threadId: string;
  messageId: string;
  text: string;
  final: boolean;
}

export interface TurnView {
  /** `null` only for the optimistic turn, until `turn.created` arrives. */
  turnId: string | null;
  input: string;
  status: TurnStatus;
  error: string | null;
  items: AssistantItem[];
  /** Event ids already applied: applying one twice changes nothing. */
  seen: ReadonlySet<string>;
}

export function optimisticTurn(input: string): TurnView {
  return { turnId: null, input, status: "running", error: null, items: [], seen: new Set() };
}

function upsert(items: AssistantItem[], next: AssistantItem): AssistantItem[] {
  const index = items.findIndex(
    (item) => item.threadId === next.threadId && item.messageId === next.messageId,
  );
  if (index < 0) return [...items, next];
  return items.map((item, i) => (i === index ? next : item));
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** One event applied to a turn. Unknown types are remembered as seen and otherwise ignored. */
export function reduceTurn(turn: TurnView, event: CoreEvent): TurnView {
  if (turn.seen.has(event.id)) return turn;
  const seen = new Set(turn.seen).add(event.id);
  const base = { ...turn, seen };
  const threadId = str(event.thread_id);
  const messageId = str(event.message_id);

  switch (event.type) {
    case "turn.created":
      return { ...base, turnId: str(event.turn_id) || turn.turnId, status: "running" };

    case "model.message": {
      const message = event.message as { content?: unknown } | null | undefined;
      const existing = turn.items.find((i) => i.threadId === threadId && i.messageId === messageId);
      if (!message) {
        // The live start marker; a message that is already final keeps its text.
        if (existing) return base;
        return {
          ...base,
          items: upsert(turn.items, {
            kind: "assistant",
            threadId,
            messageId,
            text: "",
            final: false,
          }),
        };
      }
      const text = str(message.content);
      // A tool-only round has an empty final message: it renders nothing, so it is removed.
      if (text === "") {
        return {
          ...base,
          items: turn.items.filter((i) => !(i.threadId === threadId && i.messageId === messageId)),
        };
      }
      return {
        ...base,
        items: upsert(turn.items, { kind: "assistant", threadId, messageId, text, final: true }),
      };
    }

    case "model.message.delta": {
      const existing = turn.items.find((i) => i.threadId === threadId && i.messageId === messageId);
      // The final text already holds everything the deltas would add.
      if (existing?.final) return base;
      const text = (existing?.text ?? "") + str(event.content);
      return {
        ...base,
        items: upsert(turn.items, { kind: "assistant", threadId, messageId, text, final: false }),
      };
    }

    case "turn.done": {
      const state = str(event.state);
      const status: TurnStatus = state === "error" || state === "cancelled" ? state : "done";
      return { ...base, status, error: typeof event.error === "string" ? event.error : null };
    }

    default:
      return base;
  }
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
  if (record.state === null && turn.status === "running") turn = { ...turn, status: "interrupted" };
  return turn;
}
