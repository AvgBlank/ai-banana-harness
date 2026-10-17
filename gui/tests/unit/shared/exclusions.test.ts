// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";

const root = join(__dirname, "../../..");

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    return /\.(ts|tsx|css|html)$/.test(entry.name) ? [path] : [];
  });
}

/** The features V1 leaves out: none may appear in the app's own code or dependencies. */
describe("excluded features", () => {
  const code = sources(join(root, "src")).map((path) => ({
    path,
    text: readFileSync(path, "utf8"),
  }));
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
    dependencies?: object;
    devDependencies?: object;
  };
  const dependencies = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });

  const absent: Array<[string, RegExp]> = [
    ["voice input", /SpeechRecognition|getUserMedia|MediaRecorder/],
    ["a global shortcut", /globalShortcut/],
    ["an update check", /autoUpdater|electron-updater|checkForUpdates/],
    ["Monaco", /monaco/i],
  ];

  for (const [name, pattern] of absent) {
    test(`there is no ${name} in the source`, () => {
      const hits = code.filter((file) => pattern.test(file.text)).map((file) => file.path);
      expect(hits).toEqual([]);
    });
  }

  test("no dependency brings Monaco, an updater or speech", () => {
    expect(dependencies.filter((name) => /monaco|updater|speech/i.test(name))).toEqual([]);
  });
});
