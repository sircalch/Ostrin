import { defineConfig } from "@playwright/test";
import os from "node:os";
import path from "node:path";

const baseURL = "http://127.0.0.1:4173/Ostrin/";

export default defineConfig({
  testDir: ".",
  testMatch: "website.spec.mjs",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: "list",
  outputDir: path.join(os.tmpdir(), "ostrin-playwright-results"),
  use: {
    baseURL,
    browserName: "chromium",
    headless: true,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "node server.mjs",
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
