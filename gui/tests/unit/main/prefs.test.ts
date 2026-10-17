// @vitest-environment node

import { readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import { MAX_RECENTS, Preferences } from "../../../src/main/prefs";
import { tempDir } from "./tmp";

const dir = () => tempDir("bh-prefs-");
const DEFAULTS = {
  theme: "dark",
  sidebarOpen: true,
  changesOpen: false,
  changesScope: "turn",
  changesWidth: 480,
  lastWorkspace: null,
  recents: [],
};

test("a missing file gives the defaults", async () => {
  expect(await new Preferences(join(dir(), "preferences.json")).get()).toEqual(DEFAULTS);
});

test("a set value is stored, private, and read back by a new instance", async () => {
  const file = join(dir(), "nested", "preferences.json");
  const saved = await new Preferences(file).set({
    theme: "light",
    changesOpen: true,
    sidebarOpen: false,
  });
  expect(saved).toEqual({ ...DEFAULTS, theme: "light", changesOpen: true, sidebarOpen: false });
  expect(statSync(file).mode & 0o777).toBe(0o600);
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual(saved);
  expect(await new Preferences(file).get()).toEqual(saved);
});

test("a corrupt or foreign file falls back to the defaults and is replaced on the next save", async () => {
  const file = join(dir(), "preferences.json");
  writeFileSync(file, "{ not json");
  const prefs = new Preferences(file);
  expect(await prefs.get()).toEqual(DEFAULTS);
  writeFileSync(file, JSON.stringify({ theme: "purple" }));
  expect(await new Preferences(file).get()).toEqual(DEFAULTS);
  await prefs.set({ theme: "system" });
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ ...DEFAULTS, theme: "system" });
});

test("unknown keys are dropped when loading and never stored", async () => {
  const file = join(dir(), "preferences.json");
  writeFileSync(file, JSON.stringify({ theme: "light", evil: "x", apiKey: "sk-secret" }));
  const prefs = new Preferences(file);
  expect(await prefs.get()).toEqual({ ...DEFAULTS, theme: "light" });
  await prefs.set({ theme: undefined, evil: "x" } as never);
  const text = readFileSync(file, "utf8");
  expect(text).not.toContain("evil");
  expect(text).not.toContain("sk-secret");
  expect(JSON.parse(text)).toEqual({ ...DEFAULTS, theme: "light" });
});

test("the renderer's patch cannot set the last workspace or the recents", async () => {
  const file = join(dir(), "preferences.json");
  const prefs = new Preferences(file);
  await prefs.set({ lastWorkspace: "/evil", recents: ["/evil"] } as never);
  expect(await prefs.get()).toEqual(DEFAULTS);
});

test("recording a workspace keeps the newest ten, once each, and sets the last one", async () => {
  const prefs = new Preferences(join(dir(), "preferences.json"));
  for (let n = 0; n < 12; n++) await prefs.recordWorkspace(`/w${n}`);
  await prefs.recordWorkspace("/w5");
  const now = await prefs.get();
  expect(now.lastWorkspace).toBe("/w5");
  expect(now.recents).toHaveLength(MAX_RECENTS);
  expect(now.recents[0]).toBe("/w5");
  expect(now.recents.filter((p) => p === "/w5")).toHaveLength(1);
  expect(now.recents).not.toContain("/w0");
  expect(await new Preferences(join(dir(), "none.json")).get()).toEqual(DEFAULTS);
});
