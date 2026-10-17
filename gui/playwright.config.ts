import { defineConfig } from "@playwright/test";

const ci = Boolean(process.env.CI);

export default defineConfig({
  testDir: "tests/e2e",
  outputDir: "test-results",
  timeout: 90_000,
  expect: { timeout: 15_000 },
  // A retry is reported as a flake, not hidden: none locally, one in CI.
  retries: ci ? 1 : 0,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: { trace: "on-first-retry" },
});
