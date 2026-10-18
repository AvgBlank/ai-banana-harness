// @vitest-environment node
import { describe, expect, test } from "vitest";
import { summarize } from "../../../src/main/trust/summary";

describe("ignored settings", () => {
  test("a workspace that sets where commands run is told it is ignored", () => {
    const text = `[tools.bash]\nrunner = "container"\n[tools.bash.container]\nimage = "x:1"\n`;
    expect(summarize(text, []).summary?.ignored).toEqual([
      "tools.bash.runner",
      "tools.bash.container.image",
    ]);
  });

  test("other tool settings are not reported as ignored", () => {
    expect(summarize(`[tools.read]\nx = 1\n`, []).summary?.ignored).toEqual([]);
  });
});
