import { describe, expect, test } from "vitest";
import { parseDiff } from "@/state/diff";
import { clipOutput, MAX_OUTPUT_BYTES, MAX_OUTPUT_LINES } from "@/state/output";

describe("parseDiff", () => {
  test("skips the headers before the first hunk and numbers the lines", () => {
    const parsed = parseDiff("--- before\n+++ after\n@@ -3,2 +3,3 @@\n keep\n-old\n+new\n+extra\n");
    expect(parsed.lines.map((l) => l.kind)).toEqual(["hunk", "ctx", "del", "add", "add"]);
    expect(parsed.lines[1]).toMatchObject({ oldNo: 3, newNo: 3, text: "keep" });
    expect(parsed.lines[2]).toMatchObject({ oldNo: 4, text: "old" });
    expect(parsed.lines[3]).toMatchObject({ newNo: 4, text: "new" });
    expect(parsed.lines[4]).toMatchObject({ newNo: 5, text: "extra" });
    expect([parsed.added, parsed.removed]).toEqual([2, 1]);
  });

  test("a removed line that starts with dashes is a removal, not a header", () => {
    const parsed = parseDiff(
      "--- before\n+++ after\n@@ -1,2 +1,1 @@\n--- a yaml line\n+++ added plus\n",
    );
    expect([parsed.added, parsed.removed]).toEqual([1, 1]);
    expect(parsed.lines.map((l) => l.kind)).toEqual(["hunk", "del", "add"]);
  });

  test("the no-newline marker is a note that is neither numbered nor counted", () => {
    const parsed = parseDiff(
      "--- before\n+++ after\n@@ -0,0 +1 @@\n+x\n\\ No newline at end of file\n",
    );
    expect(parsed.lines.map((l) => l.kind)).toEqual(["hunk", "add", "note"]);
    expect(parsed.lines[2]).toMatchObject({ text: "No newline at end of file" });
    expect(parsed.lines[2]?.newNo).toBeUndefined();
    expect([parsed.added, parsed.removed]).toEqual([1, 0]);
  });

  test("several hunks restart the numbering, and text without a hunk yields nothing", () => {
    const parsed = parseDiff("@@ -1 +1 @@\n-a\n+b\n@@ -10 +10 @@\n-c\n+d\n");
    expect(parsed.lines.filter((l) => l.kind === "hunk")).toHaveLength(2);
    expect(parsed.lines[4]).toMatchObject({ oldNo: 10 });
    expect(parseDiff("just some text\nwithout hunks\n")).toEqual({
      lines: [],
      added: 0,
      removed: 0,
    });
    expect(parseDiff("")).toEqual({ lines: [], added: 0, removed: 0 });
  });

  test("a blank context line keeps its place", () => {
    const parsed = parseDiff("@@ -1,3 +1,3 @@\n a\n\n-b\n+c\n");
    expect(parsed.lines[2]).toMatchObject({ kind: "ctx", oldNo: 2, newNo: 2, text: "" });
  });
});

describe("clipOutput", () => {
  test("short output is shown whole", () => {
    expect(clipOutput("a\nb")).toEqual({ text: "a\nb", clipped: false });
  });

  test("more than 200 lines is cut at 200", () => {
    const text = Array.from({ length: MAX_OUTPUT_LINES + 50 }, (_, i) => `line ${i}`).join("\n");
    const clip = clipOutput(text);
    expect(clip.clipped).toBe(true);
    expect(clip.text.split("\n")).toHaveLength(MAX_OUTPUT_LINES);
    expect(clipOutput(text.split("\n").slice(0, MAX_OUTPUT_LINES).join("\n")).clipped).toBe(false);
  });

  test("more than 64 KiB is cut within the limit, on a character boundary", () => {
    const clip = clipOutput("é".repeat(MAX_OUTPUT_BYTES));
    expect(clip.clipped).toBe(true);
    expect(new TextEncoder().encode(clip.text).length).toBeLessThanOrEqual(MAX_OUTPUT_BYTES);
    expect(clipOutput("a".repeat(MAX_OUTPUT_BYTES)).clipped).toBe(false);
  });

  test("the cut keeps the longest prefix that fits, whatever the characters (a split emoji is kept as before)", () => {
    for (const unit of ["é", "€", "😀", "a😀é"]) {
      const clip = clipOutput(`a${unit.repeat(MAX_OUTPUT_BYTES)}`);
      const bytes = new TextEncoder().encode(clip.text).length;
      expect(bytes).toBeLessThanOrEqual(MAX_OUTPUT_BYTES);
      // The next character would not have fit: at most 4 bytes are left over.
      expect(bytes).toBeGreaterThan(MAX_OUTPUT_BYTES - 4);
    }
  });
});
