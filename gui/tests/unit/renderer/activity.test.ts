import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import {
  type CoreEvent,
  groupStatus,
  optimisticTurn,
  reduceTurn,
  summarize,
  type ToolItem,
  type TurnView,
  turnFromRecord,
  ulidMs,
} from "../../../src/renderer/state/reduceTurn";

let n = 0;
const ev = (type: string, rest: Record<string, unknown> = {}): CoreEvent => ({
  id: `e${++n}`,
  type,
  ...rest,
});
const usage = { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 };
const call = (thread: string, id: string, name: string, args: unknown) =>
  ev("tool.call", {
    thread_id: thread,
    tool_call_id: id,
    name,
    arguments: typeof args === "string" ? args : JSON.stringify(args),
  });
const response = (
  thread: string,
  id: string,
  content: string,
  extra: Record<string, unknown> = {},
) =>
  ev("tool.response", { thread_id: thread, tool_call_id: id, content, blocked: false, ...extra });
const text = (thread: string, message: string, content: string) =>
  ev("model.message", {
    thread_id: thread,
    message_id: message,
    message: { content, finish_reason: "stop", usage },
  });
const created = () => ev("turn.created", { session_id: "s", turn_id: "T" });
const done = (state = "done") => ev("turn.done", { state });
const run = (events: CoreEvent[]) => events.reduce(reduceTurn, optimisticTurn("go"));
const tools = (turn: TurnView) => turn.items.filter((i): i is ToolItem => i.kind === "tool");
const strip = (t: TurnView) => ({ ...t, seen: undefined });

const DIFF = "--- before\n+++ after\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n";

describe("tool cards", () => {
  test("a call appears running and its response updates the same card in place", () => {
    const start = run([created(), call("t", "c1", "read_file", { path: "a.txt" })]);
    expect(tools(start)).toHaveLength(1);
    expect(tools(start)[0]).toMatchObject({
      state: "running",
      name: "read_file",
      args: { path: "a.txt" },
    });
    const end = [response("t", "c1", "     1\thello")].reduce(reduceTurn, start);
    expect(tools(end)).toHaveLength(1);
    expect(tools(end)[0]).toMatchObject({ state: "done", output: "     1\thello" });
  });

  test("a parallel round shows running together and resolves independently", () => {
    const events = [
      call("t", "a", "grep", { pattern: "x" }),
      call("t", "b", "glob", { pattern: "*.rs" }),
    ];
    const mid = run(events);
    expect(tools(mid).map((t) => t.state)).toEqual(["running", "running"]);
    const after = reduceTurn(mid, response("t", "b", "src/a.rs"));
    expect(tools(after).map((t) => t.state)).toEqual(["running", "done"]);
  });

  test("bash output loses its exit-code line and the code becomes a chip value", () => {
    const turn = run([
      call("t", "c", "bash", { command: "make test" }),
      response("t", "c", "exit code: 101\nerror: boom\nmore"),
    ]);
    expect(tools(turn)[0]).toMatchObject({
      state: "done",
      exitCode: 101,
      output: "error: boom\nmore",
    });
    const ok = run([
      call("t", "c", "bash", { command: "true" }),
      response("t", "c", "exit code: 0"),
    ]);
    expect(tools(ok)[0]).toMatchObject({ exitCode: 0, output: "" });
  });

  test("only bash output is parsed for an exit code", () => {
    const turn = run([
      call("t", "c", "read_file", { path: "x" }),
      response("t", "c", "exit code: 3\nfile text"),
    ]);
    expect(tools(turn)[0]?.exitCode).toBeUndefined();
    expect(tools(turn)[0]?.output).toBe("exit code: 3\nfile text");
  });

  test("a blocked call carries the guardrail rule and never reads as done", () => {
    const turn = run([
      call("t", "c", "bash", { command: "git push" }),
      response("t", "c", 'Blocked by guardrail: bash rule "git push*"', { blocked: true }),
    ]);
    expect(tools(turn)[0]).toMatchObject({ state: "blocked", rule: 'bash rule "git push*"' });
  });

  test("argument text that is not a JSON object keeps the card with the raw text", () => {
    for (const raw of ["{not json", "[1,2]", "42", ""]) {
      const turn = run([call("t", "c", "bash", raw)]);
      expect(tools(turn)[0]).toMatchObject({ args: null, rawArgs: raw, state: "running" });
    }
  });

  test("edit and write_file responses carry the parsed diff counts", () => {
    const turn = run([
      call("t", "c", "edit", { path: "src/a.rs" }),
      response("t", "c", "Edited", { diff: DIFF }),
    ]);
    const card = tools(turn)[0];
    expect([card?.diff?.added, card?.diff?.removed]).toEqual([1, 1]);
  });

  test("a response for a call that was never seen changes nothing", () => {
    const turn = run([created()]);
    expect(strip(reduceTurn(turn, response("t", "ghost", "x")))).toEqual({
      ...strip(turn),
      seen: undefined,
    });
  });
});

describe("subagents", () => {
  const parent = (id: string, name: string) =>
    call("root", id, "create_sub_agent", { name, input: `do ${name}` });
  const thread = (id: string, title: string) =>
    ev("thread.created", { thread_id: id, parent: "root", title });

  test("threads nest in the card with the matching name, whatever the arrival order", () => {
    const turn = run([
      created(),
      parent("pa", "alpha"),
      parent("pb", "beta"),
      thread("tb", "beta"),
      thread("ta", "alpha"),
      text("ta", "m", "alpha says hi"),
      text("tb", "m", "beta says hi"),
    ]);
    const [a, b] = tools(turn);
    expect(a?.children).toEqual(["ta"]);
    expect(b?.children).toEqual(["tb"]);
    expect(turn.threads.ta?.items).toHaveLength(1);
    expect(turn.threads.tb?.items).toHaveLength(1);
  });

  test("no child's items reach the root list", () => {
    const turn = run([
      parent("pa", "alpha"),
      thread("ta", "alpha"),
      text("ta", "m", "child text"),
      call("ta", "c", "bash", { command: "ls" }),
    ]);
    expect(turn.items.map((i) => i.kind)).toEqual(["tool"]);
    expect(turn.threads.ta?.items.map((i) => i.kind)).toEqual(["assistant", "tool"]);
  });

  test("a title that matches no call goes to the oldest unclaimed one", () => {
    const turn = run([parent("pa", "alpha"), parent("pb", "beta"), thread("t1", "something else")]);
    expect(tools(turn)[0]?.children).toEqual(["t1"]);
    expect(tools(turn)[1]?.children).toEqual([]);
  });

  test("a thread with no subagent call still gets a thread record", () => {
    const turn = run([thread("t1", "orphan")]);
    expect(turn.threads.t1?.title).toBe("orphan");
    expect(turn.items).toEqual([]);
  });

  test("the group status is error over running over done", () => {
    const base = run([parent("pa", "alpha"), thread("t1", "alpha")]);
    const card = () => tools(base)[0] as ToolItem;
    expect(groupStatus(base, card())).toBe("running");
    const oneDone = reduceTurn(base, ev("thread.done", { thread_id: "t1" }));
    expect(groupStatus(oneDone, tools(oneDone)[0] as ToolItem)).toBe("done");
    const failed = reduceTurn(base, ev("thread.done", { thread_id: "t1", error: "it broke" }));
    expect(groupStatus(failed, tools(failed)[0] as ToolItem)).toBe("error");
    expect(failed.threads.t1?.error).toBe("it broke");
    // Two children: one failed and one running is an error; one done and one running is running.
    const two = run([
      parent("pa", "alpha"),
      parent("pb", "beta"),
      thread("t1", "alpha"),
      thread("t2", "beta"),
      ev("thread.done", { thread_id: "t1" }),
    ]);
    expect(groupStatus(two, tools(two)[0] as ToolItem)).toBe("done");
  });
});

describe("notices, interruption and the summary", () => {
  test("context.compacted adds a notice in its thread", () => {
    const turn = run([ev("context.compacted", { thread_id: "t", usage })]);
    expect(turn.items).toEqual([
      expect.objectContaining({ kind: "notice", text: "Context compacted · 10 tokens" }),
    ]);
  });

  test("a stored turn with no state and no done marks running tools and threads interrupted", () => {
    const events = [
      created(),
      call("root", "p", "create_sub_agent", { name: "a" }),
      ev("thread.created", { thread_id: "t1", parent: "root", title: "a" }),
      call("t1", "c", "bash", { command: "sleep 9" }),
    ];
    const turn = turnFromRecord({ turn_id: "T", input: "go", state: null, error: null, events });
    expect(turn.status).toBe("interrupted");
    expect(tools(turn)[0]?.state).toBe("interrupted");
    expect((turn.threads.t1?.items[0] as ToolItem | undefined)?.state).toBe("interrupted");
    expect(turn.threads.t1?.status).toBe("interrupted");
    expect(groupStatus(turn, tools(turn)[0] as ToolItem)).toBe("interrupted");
  });

  test("a finished stored turn keeps its states", () => {
    const events = [
      created(),
      call("t", "c", "bash", { command: "ls" }),
      response("t", "c", "exit code: 0\nok"),
      done(),
    ];
    expect(
      tools(turnFromRecord({ turn_id: "T", input: "", state: "done", error: null, events }))[0]
        ?.state,
    ).toBe("done");
  });

  test("the summary counts a file once across edits, sums the lines and ignores bash and failures", () => {
    const turn = run([
      created(),
      call("t", "1", "edit", { path: "a.rs" }),
      response("t", "1", "ok", { diff: DIFF }),
      call("t", "2", "write_file", { path: "b.rs" }),
      response("t", "2", "ok", { diff: "--- before\n+++ after\n@@ -0,0 +1,3 @@\n+1\n+2\n+3\n" }),
      call("t", "3", "edit", { path: "a.rs" }),
      response("t", "3", "ok", { diff: DIFF }),
      call("t", "4", "bash", { command: "touch c" }),
      response("t", "4", "exit code: 0"),
      call("t", "5", "edit", { path: "d.rs" }),
      response("t", "5", 'Blocked by guardrail: edit rule "x"', { blocked: true, diff: DIFF }),
      done(),
    ]);
    const summary = summarize(turn);
    expect(summary.files).toEqual([
      { path: "a.rs", added: 2, removed: 2 },
      { path: "b.rs", added: 3, removed: 0 },
    ]);
    expect([summary.added, summary.removed]).toEqual([5, 2]);
  });

  test("a diff on any other tool is not counted as an edit", () => {
    const turn = run([
      call("t", "1", "bash", { command: "x" }),
      response("t", "1", "exit code: 0", { diff: DIFF }),
    ]);
    expect(summarize(turn).files).toEqual([]);
  });

  test("edits made inside a subagent are in the summary", () => {
    const turn = run([
      call("root", "p", "create_sub_agent", { name: "a" }),
      ev("thread.created", { thread_id: "t1", parent: "root", title: "a" }),
      call("t1", "c", "edit", { path: "x.rs" }),
      response("t1", "c", "ok", { diff: DIFF }),
    ]);
    expect(summarize(turn).files).toEqual([{ path: "x.rs", added: 1, removed: 1 }]);
  });

  test("the duration is the time between two event ids, and absent for ids that are not ULIDs", () => {
    const at = (ms: number) => {
      const alphabet = "0123456789abcdefghjkmnpqrstvwxyz";
      let t = ms;
      let out = "";
      for (let i = 0; i < 10; i++) {
        out = alphabet[t % 32] + out;
        t = Math.floor(t / 32);
      }
      return `${out}${"0".repeat(16)}`;
    };
    expect(ulidMs(at(1_700_000_000_000))).toBe(1_700_000_000_000);
    expect(ulidMs("e1")).toBeNull();
    const turn = [
      { id: at(1_700_000_000_000), type: "turn.created", turn_id: "T" },
      { id: at(1_700_000_011_000), type: "turn.done", state: "done" },
    ].reduce(reduceTurn, optimisticTurn("x"));
    expect(summarize(turn).durationMs).toBe(11_000);
    expect(summarize(run([created(), done()])).durationMs).toBeNull();
  });
});

describe("replay equivalence", () => {
  const frames = readFileSync(
    join(__dirname, "../../../../crates/harness-bridge/tests/fixtures/turns.ndjson"),
    "utf8",
  )
    .split("\n")
    .filter(Boolean)
    .map((l) => JSON.parse(l));

  function scenarioEvents(name: string): CoreEvent[] {
    const start = frames.findIndex((f) => f.scenario === name);
    const next = frames.findIndex((f, i) => i > start && f.scenario !== undefined);
    return frames
      .slice(start, next < 0 ? undefined : next)
      .filter((f) => f.kind === "event")
      .map((f) => f.event as CoreEvent);
  }
  /** What the database keeps: no deltas, no start markers. */
  const persisted = (events: CoreEvent[]) =>
    events.filter(
      (e) => e.type !== "model.message.delta" && !(e.type === "model.message" && !("message" in e)),
    );

  for (const name of ["plain-answer", "edit-and-approve", "single-subagent", "compaction"]) {
    test(`the live stream and the stored events of ${name} give the same view`, () => {
      const events = scenarioEvents(name);
      expect(events.length).toBeGreaterThan(2);
      const live = events.reduce(reduceTurn, optimisticTurn("go"));
      const replayed = turnFromRecord({
        turn_id: "T",
        input: "go",
        state: "done",
        error: null,
        events: persisted(events),
      });
      expect(strip({ ...replayed, turnId: live.turnId })).toEqual(strip(live));
    });
  }

  test("single-subagent nests the child in the card and keeps it out of the root", () => {
    const turn = scenarioEvents("single-subagent").reduce(reduceTurn, optimisticTurn("go"));
    const card = tools(turn).find((t) => t.name === "create_sub_agent") as ToolItem;
    expect(card.children).toHaveLength(1);
    expect(Object.keys(turn.threads)).toHaveLength(1);
    expect(turn.items.filter((i) => i.kind === "assistant")).toHaveLength(1);
    expect(groupStatus(turn, card)).toBe("done");
  });

  test("edit-and-approve produces tool cards with an approval-free done state", () => {
    const turn = scenarioEvents("edit-and-approve").reduce(reduceTurn, optimisticTurn("go"));
    expect(tools(turn).some((t) => t.name === "write_file" && t.state === "done" && t.diff)).toBe(
      true,
    );
  });

  test("a bash-and-test shaped turn replays to the same view with its exit chip", () => {
    const events = [
      created(),
      ev("model.message", { thread_id: "t", message_id: "m1" }),
      ev("model.message", {
        thread_id: "t",
        message_id: "m1",
        message: { content: "", finish_reason: "tool_calls", usage },
      }),
      call("t", "c1", "bash", { command: "sh check.sh" }),
      response("t", "c1", "exit code: 0\nok"),
      ev("model.message", { thread_id: "t", message_id: "m2" }),
      ev("model.message.delta", {
        thread_id: "t",
        message_id: "m2",
        message_id_: 0,
        content: "The check printed ok.",
      }),
      ev("model.message", {
        thread_id: "t",
        message_id: "m2",
        message: { content: "The check printed ok.", finish_reason: "stop", usage },
      }),
      done(),
    ];
    const live = events.reduce(reduceTurn, optimisticTurn("go"));
    const replayed = turnFromRecord({
      turn_id: "T",
      input: "go",
      state: "done",
      error: null,
      events: persisted(events),
    });
    expect(strip({ ...replayed, turnId: live.turnId })).toEqual(strip(live));
    expect(tools(live)[0]).toMatchObject({ exitCode: 0, output: "ok", state: "done" });
  });
});
