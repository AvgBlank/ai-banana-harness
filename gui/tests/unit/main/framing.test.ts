// @vitest-environment node
import { expect, test } from "vitest";
import { timeoutFor } from "../../../src/main/sidecar/client";
import { LineSplitter, parseFrame } from "../../../src/main/sidecar/framing";

const EIGHT_MIB = 8 * 1024 * 1024;

test("lines split across chunks, with several lines in one chunk", () => {
  const splitter = new LineSplitter();
  expect(splitter.push(Buffer.from("ab"))).toEqual([]);
  expect(splitter.push(Buffer.from("c\nd\ne"))).toEqual([
    { kind: "line", text: "abc" },
    { kind: "line", text: "d" },
  ]);
  expect(splitter.push(Buffer.from("\n"))).toEqual([{ kind: "line", text: "e" }]);
});

test("a line of exactly 8 MiB is kept and one byte more is discarded to its newline", () => {
  const splitter = new LineSplitter();
  const [kept] = splitter.push(Buffer.concat([Buffer.alloc(EIGHT_MIB, 0x61), Buffer.from("\n")]));
  expect(kept?.kind).toBe("line");
  const over = [
    ...splitter.push(Buffer.alloc(EIGHT_MIB + 1, 0x61)),
    ...splitter.push(Buffer.from("still the same line")),
    ...splitter.push(Buffer.from("\nnext\n")),
  ];
  expect(over).toEqual([
    { kind: "dropped", reason: "oversized" },
    { kind: "line", text: "next" },
  ]);
});

test("invalid UTF-8 is dropped without losing the lines around it", () => {
  const splitter = new LineSplitter();
  expect(splitter.push(Buffer.from([0x61, 0x0a, 0xc3, 0x28, 0x0a, 0x62, 0x0a]))).toEqual([
    { kind: "line", text: "a" },
    { kind: "dropped", reason: "invalid_utf8" },
    { kind: "line", text: "b" },
  ]);
});

test("parseFrame accepts protocol frames and nothing else", () => {
  expect(
    parseFrame('{"kind":"ready","protocol":1,"app":"a","version":"v","api_key_present":true}'),
  ).not.toBeNull();
  expect(parseFrame("")).toBeNull();
  expect(parseFrame("[]")).toBeNull();
  expect(parseFrame('{"kind":"request","id":"1","method":"x","params":{}}')).toBeNull();
});

test("request timeouts: 60 s for open, resume, undo and changes, else 30 s", () => {
  const long = [
    "workspace.open",
    "session.resume",
    "session.undo",
    "changes.workspace",
    "changes.turn",
  ];
  for (const method of long) expect(timeoutFor(method), method).toBe(60_000);
  for (const method of [
    "session.list",
    "turn.start",
    "turn.cancel",
    "action.reply",
    "provider.list",
  ]) {
    expect(timeoutFor(method), method).toBe(30_000);
  }
});
