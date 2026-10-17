import { defineConfig } from "@playwright/test";

// The packaged-app smoke test only: `make gui-package-smoke`, after `make gui-package`.
export default defineConfig({
  testDir: "tests/e2e",
  testMatch: "packaged.spec.ts",
  outputDir: "test-results",
  timeout: 90_000,
  expect: { timeout: 15_000 },
  retries: 0,
  workers: 1,
  reporter: [["list"]],
});
