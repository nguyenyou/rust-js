// Vitest's browser mode, with Playwright, for rust-js's `#[test]`s (ADR 0027):
//
//   RUST_JS_TESTS=out/app.test.js bunx --bun vitest run -c browser/vitest.config.ts
//
// Vitest runs each test file inside the browser page, and its `globals`
// provide the `test()` that `rust-js --test` files call.

import { fileURLToPath } from "node:url";

import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

const files = (process.env.RUST_JS_TESTS ?? "").split(/[\s,]+/).filter(Boolean);
if (files.length === 0) {
  throw new Error("set RUST_JS_TESTS to the .test.js files that `rust-js --test` wrote");
}

export default defineConfig({
  root: fileURLToPath(new URL("..", import.meta.url)),
  test: {
    globals: true,
    include: files,
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }, { browser: "firefox" }, { browser: "webkit" }],
    },
  },
});
