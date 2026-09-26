// Playwright Test, on Bun, for rust-js's `#[test]`s (ADR 0027). See rust-tests.spec.ts.
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "rust-tests.spec.ts",
  fullyParallel: true,
  reporter: [["list"]],
  // One run, three engines.
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
});
