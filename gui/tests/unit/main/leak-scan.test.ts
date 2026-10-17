// @vitest-environment node
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import { findLeaks, TEST_KEY } from "../../../scripts/scan-test-results.mjs";
import { tempDir } from "./tmp";

function results(files: Record<string, string | Buffer>): string {
  const root = tempDir("scan-");
  for (const [name, content] of Object.entries(files)) {
    const path = join(root, name);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, content);
  }
  return root;
}

// What Playwright writes for a failing test: a snapshot, then the test's source with line numbers.
const EXCERPT = `# Test source\n\n\`\`\`ts\n  5   | \n> 6   | const TEST_KEY = "${TEST_KEY}";\n  7   | \n\`\`\`\n`;

describe("findLeaks", () => {
  test("the test key in a runtime log is a leak", () => {
    const dir = results({ "a/main-process.log": `sidecar: started with ${TEST_KEY}\n` });
    expect(findLeaks(dir)).toEqual([
      { file: join(dir, "a/main-process.log"), line: `sidecar: started with ${TEST_KEY}` },
    ]);
  });

  test("the numbered source excerpt in error-context.md is not a leak", () => {
    expect(findLeaks(results({ "a/error-context.md": EXCERPT }))).toEqual([]);
  });

  test("the key anywhere else in error-context.md still is: a page snapshot or an error message", () => {
    const dir = results({
      "a/error-context.md": `${EXCERPT}\n# Page snapshot\n- textbox: ${TEST_KEY}\nError: saw ${TEST_KEY}\n`,
    });
    expect(findLeaks(dir).map((l) => l.line)).toEqual([
      `- textbox: ${TEST_KEY}`,
      `Error: saw ${TEST_KEY}`,
    ]);
  });

  test("a numbered line is skipped only in error-context.md", () => {
    const dir = results({ "a/notes.md": `  6   | const TEST_KEY = "${TEST_KEY}";\n` });
    expect(findLeaks(dir)).toHaveLength(1);
  });

  test("a clean directory, a missing one and binary files report nothing", () => {
    expect(findLeaks(results({ "a/log.txt": "nothing here\n" }))).toEqual([]);
    expect(findLeaks(join(tempDir("scan-"), "missing"))).toEqual([]);
    expect(
      findLeaks(
        results({ "a/trace.zip": Buffer.concat([Buffer.from([0]), Buffer.from(TEST_KEY)]) }),
      ),
    ).toEqual([]);
  });

  test("another key string is searched for when given", () => {
    expect(findLeaks(results({ "a.log": "secret-xyz\n" }), "secret-xyz")).toHaveLength(1);
  });
});
