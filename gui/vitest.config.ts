import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": resolve("src/renderer") } },
  test: {
    environment: "jsdom",
    setupFiles: ["tests/unit/renderer/setup.ts"],
    include: ["tests/unit/**/*.test.{ts,tsx}"],
  },
});
