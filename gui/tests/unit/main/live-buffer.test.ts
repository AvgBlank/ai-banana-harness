// @vitest-environment node
import { describe, expect, test } from "vitest";
import { LiveBuffers, MAX_BUFFER_BYTES } from "../../../src/main/live/buffer";
import type { EventFrame } from "../../../src/shared/protocol";

let n = 0;
function frame(
  workspace: string,
  session: string,
  turn: string | null,
  type: string,
  extra: Record<string, unknown> = {},
): EventFrame {
  return {
    kind: "event",
    workspace,
    session_id: session,
    turn_id: turn,
    event: { id: `e${++n}`, type, ...extra },
  } as EventFrame;
}

describe("LiveBuffers", () => {
  test("a running turn is attached with its frames and id", () => {
    const live = new LiveBuffers();
    const a = frame("/w", "s1", "T1", "turn.created", { session_id: "s1", turn_id: "T1" });
    const b = frame("/w", "s1", "T1", "model.message.delta", { thread_id: "t" });
    live.push(a);
    live.push(b);
    expect(live.attach("/w", "s1")).toEqual({ frames: [a, b], activeTurnId: "T1" });
    expect(live.running).toBe(true);
  });

  test("a finished turn stays attachable, with no running turn, until the next one", () => {
    const live = new LiveBuffers();
    const created = frame("/w", "s1", "T1", "turn.created", { session_id: "s1", turn_id: "T1" });
    const done = frame("/w", "s1", "T1", "turn.done", { state: "done" });
    live.push(created);
    live.push(done);
    expect(live.attach("/w", "s1")).toEqual({ frames: [created, done], activeTurnId: null });
    expect(live.running).toBe(false);

    const next = frame("/w", "s1", "T2", "turn.created", { session_id: "s1", turn_id: "T2" });
    live.push(next);
    expect(live.attach("/w", "s1")).toEqual({ frames: [next], activeTurnId: "T2" });
  });

  test("workspaces and sessions are kept apart", () => {
    const live = new LiveBuffers();
    live.push(frame("/a", "s1", "T1", "turn.created", { session_id: "s1", turn_id: "T1" }));
    live.push(frame("/b", "s2", "T2", "turn.created", { session_id: "s2", turn_id: "T2" }));
    expect(live.attach("/a", "s2")).toEqual({ frames: [], activeTurnId: null });
    expect(live.attach("/b", "s2").activeTurnId).toBe("T2");
    live.clear("/a");
    expect(live.attach("/a", "s1").frames).toEqual([]);
    expect(live.attach("/b", "s2").frames).toHaveLength(1);
    live.clear();
    expect(live.running).toBe(false);
  });

  test("past about 5 MiB the oldest frames go and the newest stay", () => {
    const live = new LiveBuffers();
    live.push(frame("/w", "s1", "T1", "turn.created", { session_id: "s1", turn_id: "T1" }));
    const big = "x".repeat(1024 * 1024);
    const sent: EventFrame[] = [];
    for (let i = 0; i < 7; i++) {
      const f = frame("/w", "s1", "T1", "model.message.delta", { text: big });
      sent.push(f);
      live.push(f);
    }
    const { frames, activeTurnId } = live.attach("/w", "s1");
    expect(activeTurnId).toBe("T1");
    expect(frames.length).toBeLessThan(8);
    expect(frames.at(-1)).toBe(sent.at(-1));
    expect(JSON.stringify(frames).length).toBeLessThanOrEqual(MAX_BUFFER_BYTES + 200);
  });

  test("a single frame over the limit is still kept", () => {
    const live = new LiveBuffers();
    const huge = frame("/w", "s1", "T1", "model.message", {
      text: "x".repeat(MAX_BUFFER_BYTES + 10),
    });
    live.push(huge);
    expect(live.attach("/w", "s1").frames).toEqual([huge]);
  });

  test("a frame for a workspace with no turn.created yet still gets a buffer", () => {
    const live = new LiveBuffers();
    const f = frame("/w", "s1", "T1", "model.message.delta");
    live.push(f);
    expect(live.attach("/w", "s1")).toEqual({ frames: [f], activeTurnId: null });
  });
});
