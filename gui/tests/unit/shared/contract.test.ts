import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import {
  eventKinds,
  methodNames,
  methods,
  requestFrame,
  sidecarFrame,
} from "../../../src/shared/protocol";

const fixtures = join(__dirname, "../../../../crates/harness-bridge/tests/fixtures");

type Line = { file: string; number: number; value: Record<string, unknown> };

function goldenFrames(): Line[] {
  return (
    readdirSync(fixtures)
      .filter((name) => name.endsWith(".ndjson"))
      .sort()
      .flatMap((file) =>
        readFileSync(join(fixtures, file), "utf8")
          .split("\n")
          .filter((line) => line.trim() !== "")
          .map((line, index) => ({ file, number: index + 1, value: JSON.parse(line) })),
      )
      // Some files lead with a `{"scenario": ...}` label line that is not a frame.
      .filter((line) => "kind" in line.value)
  );
}

function where(line: Line): string {
  return `${line.file}:${line.number}`;
}

function collectTypes(value: unknown, found: Set<string>): void {
  if (Array.isArray(value)) for (const item of value) collectTypes(item, found);
  else if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      if (key === "type" && typeof item === "string") found.add(item);
      collectTypes(item, found);
    }
  }
}

test("there are golden frames to check", () => {
  expect(goldenFrames().length).toBeGreaterThan(100);
});

test("every golden frame parses with the schemas", () => {
  for (const line of goldenFrames()) {
    const schema = line.value.kind === "request" ? requestFrame : sidecarFrame;
    const parsed = schema.safeParse(line.value);
    expect(parsed.success, `${where(line)}: ${parsed.error?.message}`).toBe(true);
  }
});

test("requests and results match their method's schemas", () => {
  const sent = new Map<string, string>();
  let checked = 0;
  for (const line of goldenFrames()) {
    const frame = line.value as { kind: string; id?: string; method?: string; ok?: boolean };
    if (frame.kind === "request" && frame.id && frame.method) {
      sent.set(frame.id, frame.method);
      const method = methods[frame.method as keyof typeof methods];
      expect(method, `${where(line)}: unknown method ${frame.method}`).toBeDefined();
      const parsed = method.params.safeParse((line.value as { params: unknown }).params);
      expect(parsed.success, `${where(line)}: params: ${parsed.error?.message}`).toBe(true);
    }
    if (frame.kind === "response" && frame.ok === true && frame.id) {
      const name = sent.get(frame.id);
      expect(name, `${where(line)}: a response with no request before it`).toBeDefined();
      const method = methods[name as keyof typeof methods];
      const parsed = method.result.safeParse((line.value as { result: unknown }).result);
      expect(parsed.success, `${where(line)}: ${name} result: ${parsed.error?.message}`).toBe(true);
      checked++;
    }
  }
  expect(checked).toBeGreaterThan(20);
});

test("every method and event kind the schemas define has a golden frame", () => {
  const methodsSeen = new Set<string>();
  const typesSeen = new Set<string>();
  for (const line of goldenFrames()) {
    if (typeof line.value.method === "string") methodsSeen.add(line.value.method);
    collectTypes(line.value, typesSeen);
  }
  expect(methodNames.filter((name) => !methodsSeen.has(name))).toEqual([]);
  expect(eventKinds.filter((kind) => !typesSeen.has(kind))).toEqual([]);
});

test("a schema rejects what the sidecar must never send", () => {
  const bad = [
    {
      kind: "event",
      workspace: "w",
      session_id: "s",
      turn_id: null,
      event: { id: "e", type: "nope" },
    },
    { kind: "ready", protocol: "1", app: "a", version: "v", api_key_present: true },
    { kind: "response", id: "1", ok: true },
    { kind: "banana" },
  ];
  for (const frame of bad) expect(sidecarFrame.safeParse(frame).success).toBe(false);
});
