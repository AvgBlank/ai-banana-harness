// Fails when the fixed test API key appears in the test results, which CI may upload.
// Playwright's `error-context.md` quotes the failing test's source with numbered lines, and that
// source names the key as a constant; those excerpt lines are not output of the app, so they are
// skipped. Every other line of that file, and every line of every other file, is scanned.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export const TEST_KEY = "sk-test-0123456789abcdef";

const SOURCE_EXCERPT = /^\s*>?\s*\d+\s+\|/;

/** Every `{ file, line }` in `dir` (recursively) that holds `key`; binary files are not read. */
export function findLeaks(dir, key = TEST_KEY) {
  const leaks = [];
  const visit = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) {
        visit(path);
        continue;
      }
      if (!entry.isFile()) continue;
      const bytes = readFileSync(path);
      if (bytes.includes(0)) continue;
      const skipExcerpts = entry.name === "error-context.md";
      for (const line of bytes.toString("utf8").split("\n")) {
        if (!line.includes(key)) continue;
        if (skipExcerpts && SOURCE_EXCERPT.test(line)) continue;
        leaks.push({ file: path, line: line.trim() });
      }
    }
  };
  try {
    visit(dir);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  return leaks;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const leaks = findLeaks(process.argv[2] ?? "test-results");
  for (const { file, line } of leaks) console.error(`${file}: ${line}`);
  if (leaks.length > 0) {
    console.error("the test key appears in the test results");
    process.exit(1);
  }
  console.log("no test key in the test results");
}
