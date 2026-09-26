// Playwright Test, on Bun, for rust-js's `#[test]`s (ADR 0027). See rust-tests.spec.ts.
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "rust-tests.spec.ts",
  fullyParallel: true,
  reporter: [["list"]],
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
});
