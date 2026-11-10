import { defineConfig } from "@playwright/test";

// Benchmark runs are separate from the test suite: they are slow, they measure, and they write evidence.
// Run with: npx playwright test --config playwright.bench.config.mjs
export default defineConfig({
  testDir: "./bench",
  testMatch: ["workbench-bench.spec.mjs"],
  workers: 1,
  retries: 0,
  reporter: "list",
  timeout: 600_000,
  use: { baseURL: "http://127.0.0.1:4173/Ostrin/", headless: true, browserName: "chromium" },
  webServer: { command: "node server.mjs", url: "http://127.0.0.1:4173/Ostrin/", reuseExistingServer: false, env: { PORT: "4173" }, timeout: 30_000 },
});
