import { describe, expect, test } from "vitest";
import { folderName, relativeTime } from "@/lib/format";
import { groupByWorkspace } from "@/state/sessions";
import { session } from "./fakes";

const NOW = Date.parse("2026-10-17T12:00:00.000Z");
const ago = (ms: number) => new Date(NOW - ms).toISOString();
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe("relativeTime", () => {
  test.each([
    [0, "now"],
    [59_000, "now"],
    [MIN, "1m"],
    [59 * MIN, "59m"],
    [HOUR, "1h"],
    [23 * HOUR, "23h"],
    [24 * HOUR, "yesterday"],
    [47 * HOUR, "yesterday"],
    [2 * DAY, "2d"],
    [6 * DAY, "6d"],
    [7 * DAY, "1w"],
    [15 * DAY, "2w"],
  ])("%d ms ago is %s", (age, text) => {
    expect(relativeTime(ago(age), NOW)).toBe(text);
  });

  test("a time in the future reads as now and junk reads as nothing", () => {
    expect(relativeTime(ago(-5 * MIN), NOW)).toBe("now");
    expect(relativeTime("not a date", NOW)).toBe("");
  });
});

describe("folderName", () => {
  test("is the last path segment", () => {
    expect(folderName("/Users/me/alpha")).toBe("alpha");
    expect(folderName("/Users/me/alpha/")).toBe("alpha");
    expect(folderName("alpha")).toBe("alpha");
    expect(folderName("/")).toBe("/");
  });
});

describe("groupByWorkspace", () => {
  test("orders groups by their newest session and sessions newest first", () => {
    const groups = groupByWorkspace([
      session({ id: "1", workspace: "/a", updated_at: "2026-10-01T00:00:00Z" }),
      session({ id: "2", workspace: "/b", updated_at: "2026-10-10T00:00:00Z" }),
      session({ id: "3", workspace: "/a", updated_at: "2026-10-15T00:00:00Z" }),
    ]);
    expect(groups.map((g) => g.workspace)).toEqual(["/a", "/b"]);
    expect(groups[0]?.sessions.map((s) => s.id)).toEqual(["3", "1"]);
  });
});
