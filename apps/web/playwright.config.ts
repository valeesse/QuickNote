import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 45_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  retries: 1,
  use: {
    baseURL: process.env.WEB_E2E_BASE_URL || "http://localhost:8081",
    trace: "retain-on-failure",
    ...devices["Desktop Chrome"],
  },
  reporter: [["list"]],
});
