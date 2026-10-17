import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import {
  type CoreEvent,
  optimisticTurn,
  reduceTurn,
  turnFromRecord,
} from "../../../src/renderer/state/reduceTurn";

const ev = (id: string, type: string, rest: Record<string, unknown> = {}): CoreEvent => ({
  id,
  type,
  ...rest,
});
const THREAD = "t1";
const usage = { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 };
const final = (id: string, text: string, message = "m1") =>
  ev(id, "model.message", {
    thread_id: THREAD,
    message_id: message,
    message: { content: text, finish_reason: "stop", usage },
  });

/** What a turn streams live: start marker, deltas, final. */
const LIVE: CoreEvent[] = [
  ev("e1", "turn.created", { session_id: "s", turn_id: "T1" }),
  ev("e2", "model.message", { thread_id: THREAD, message_id: "m1" }),
  ev("e3", "model.message.delta", { thread_id: THREAD, message_id: "m1", content: "Hello" }),
  ev("e4", "model.message.delta", { thread_id: THREAD, message_id: "m1", content: " there!" }),
  final("e5", "Hello there!"),
  ev("e6", "turn.done", { state: "done" }),
];
/** What is persisted: no start marker, no deltas. */
const PERSISTED = LIVE.filter((e) => e.type !== "model.message.delta" && !(e.id === "e2"));

const run = (events: CoreEvent[], from = optimisticTurn("say hi")) =>
  events.reduce(reduceTurn, from);
const strip = (t: ReturnType<typeof run>) => ({ ...t, seen: undefined });

describe("reduceTurn", () => {
  test("a live stream reduces to the same state as the persisted events (apart from seen)", () => {
    const live = run(LIVE);
    expect(live.items).toEqual([
      { kind: "assistant", threadId: THREAD, messageId: "m1", text: "Hello there!", final: true },
    ]);
    expect(live.status).toBe("done");
    expect(strip(live)).toEqual(strip(run(PERSISTED)));
    expect(live.seen.size).toBe(LIVE.length);
    expect(run(PERSISTED).seen.size).toBe(PERSISTED.length);
  });

  test("the optimistic turn is bound by turn.created and keeps its input", () => {
    const turn = run(LIVE.slice(0, 1));
    expect(turn).toMatchObject({ turnId: "T1", input: "say hi", status: "running" });
    expect(optimisticTurn("x").turnId).toBeNull();
  });

  test("a final message with no start marker creates the message", () => {
    const turn = run([final("e5", "Only the final")]);
    expect(turn.items).toHaveLength(1);
    expect(turn.items[0]).toMatchObject({ text: "Only the final", final: true });
  });

  test("a delta without a start marker creates the message too", () => {
    const turn = run([
      ev("d", "model.message.delta", { thread_id: THREAD, message_id: "m1", content: "Hi" }),
    ]);
    expect(turn.items[0]).toMatchObject({ text: "Hi", final: false });
  });

  test("applying events twice, or the whole stream twice, gives the same state", () => {
    const once = run(LIVE);
    expect(run(LIVE, once)).toBe(once);
    const doubled = run([...LIVE.slice(0, 4), ...LIVE.slice(2, 4), ...LIVE.slice(4)]);
    expect(strip(doubled)).toEqual(strip(once));
  });

  test("deltas after the final, or overlapping a resumed prefix, do not change the text", () => {
    const resumed = run(PERSISTED);
    const replayedLive = run(LIVE.slice(2, 4), resumed);
    expect(replayedLive.items).toEqual(resumed.items);
    // A partial prefix first, then the authoritative final repairs missing deltas.
    const partial = run([LIVE[1] as CoreEvent, LIVE[3] as CoreEvent, final("e5", "Hello there!")]);
    expect(partial.items[0]?.text).toBe("Hello there!");
  });

  test("an empty final message is removed so a tool-only round renders nothing", () => {
    const turn = run([
      ev("a", "model.message", { thread_id: THREAD, message_id: "m1" }),
      final("b", ""),
    ]);
    expect(turn.items).toEqual([]);
  });

  test("turn.done maps the state and keeps the error text", () => {
    expect(run([ev("d", "turn.done", { state: "error", error: "provider 401" })])).toMatchObject({
      status: "error",
      error: "provider 401",
    });
    expect(run([ev("d", "turn.done", { state: "cancelled" })]).status).toBe("cancelled");
  });

  test("unknown event types are ignored but remembered", () => {
    const turn = run([ev("u", "future.thing", { x: 1 })]);
    expect(turn.items).toEqual([]);
    expect(turn.seen.has("u")).toBe(true);
  });

  test("two threads and two messages stay separate", () => {
    const turn = run([
      final("a", "one", "m1"),
      ev("b", "model.message", {
        thread_id: "t2",
        message_id: "m1",
        message: { content: "two", finish_reason: "stop", usage },
      }),
    ]);
    expect(turn.items.map((i) => i.text)).toEqual(["one", "two"]);
  });
});

describe("turnFromRecord", () => {
  const record = (state: string | null, events: CoreEvent[]) => ({
    turn_id: "T1",
    input: "say hi",
    state,
    error: null,
    events,
  });

  test("a finished record equals the live stream", () => {
    expect(strip(turnFromRecord(record("done", PERSISTED)))).toEqual(
      strip(run(LIVE, { ...optimisticTurn("say hi"), turnId: "T1" })),
    );
  });

  test("a record with no state is interrupted, and a finished one never is", () => {
    expect(turnFromRecord(record(null, PERSISTED.slice(0, 2))).status).toBe("interrupted");
    expect(turnFromRecord(record("done", PERSISTED)).status).toBe("done");
  });

  test("the bridge's own plain-answer golden frames reduce to the answer", () => {
    const file = join(__dirname, "../../../../crates/harness-bridge/tests/fixtures/turns.ndjson");
    const frames = readFileSync(file, "utf8")
      .split("\n")
      .filter(Boolean)
      .map((l) => JSON.parse(l));
    const start = frames.findIndex((f) => f.scenario === "plain-answer");
    const next = frames.findIndex((f, i) => i > start && f.scenario !== undefined);
    const events = frames
      .slice(start, next)
      .filter((f) => f.kind === "event")
      .map((f) => f.event as CoreEvent);
    const turn = events.reduce(reduceTurn, optimisticTurn("say hi"));
    expect(turn.status).toBe("done");
    expect(turn.items.map((i) => i.text)).toEqual(["Hello there!"]);
  });
});
