// @vitest-environment node
import { mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "vitest";
import { Preferences } from "../../../src/main/prefs";

const dir = () => mkdtempSync(join(tmpdir(), "bh-prefs-"));

test("a missing file gives the defaults", async () => {
  expect(await new Preferences(join(dir(), "preferences.json")).get()).toEqual({ theme: "dark" });
});

test("a set value is stored, private, and read back by a new instance", async () => {
  const file = join(dir(), "nested", "preferences.json");
  const saved = await new Preferences(file).set({ theme: "light" });
  expect(saved).toEqual({ theme: "light" });
  expect(statSync(file).mode & 0o777).toBe(0o600);
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ theme: "light" });
  expect(await new Preferences(file).get()).toEqual({ theme: "light" });
});

test("a corrupt or foreign file falls back to the defaults and is replaced on the next save", async () => {
  const file = join(dir(), "preferences.json");
  writeFileSync(file, "{ not json");
  const prefs = new Preferences(file);
  expect(await prefs.get()).toEqual({ theme: "dark" });
  writeFileSync(file, JSON.stringify({ theme: "purple" }));
  expect(await new Preferences(file).get()).toEqual({ theme: "dark" });
  await prefs.set({ theme: "system" });
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ theme: "system" });
});

test("an empty patch changes nothing and unknown keys are not stored", async () => {
  const file = join(dir(), "preferences.json");
  const prefs = new Preferences(file);
  await prefs.set({ theme: "light" });
  await prefs.set({} as never);
  await prefs.set({ theme: undefined, evil: "x" } as never);
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ theme: "light" });
});
