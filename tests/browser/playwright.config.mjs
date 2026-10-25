import { defineConfig } from "@playwright/test";
import os from "node:os";
import path from "node:path";

const testPort = Number(process.env.OSTRIN_TEST_PORT ?? process.env.PORT ?? 4173);
if (!Number.isInteger(testPort) || testPort < 1 || testPort > 65535) {
  throw new Error("OSTRIN_TEST_PORT must be an integer from 1 to 65535");
}
const baseURL = `http://127.0.0.1:${testPort}/Ostrin/`;

export default defineConfig({
  testDir: ".",
  testMatch: ["website.spec.mjs", "visual.spec.mjs"],
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: "list",
  outputDir: path.join(os.tmpdir(), "ostrin-playwright-results"),
  use: {
    baseURL,
    headless: true,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      testMatch: ["website.spec.mjs", "visual.spec.mjs"],
      use: { browserName: "chromium" },
    },
    {
      name: "firefox-smoke",
      testMatch: "cross-browser.spec.mjs",
      use: { browserName: "firefox" },
    },
    {
      name: "webkit-smoke",
      testMatch: "cross-browser.spec.mjs",
      use: { browserName: "webkit" },
    },
  ],
  webServer: {
    command: "node server.mjs",
    url: baseURL,
    reuseExistingServer: !process.env.CI && process.env.OSTRIN_REUSE_SERVER === "1",
    env: { PORT: String(testPort) },
    timeout: 30_000,
  },
});
