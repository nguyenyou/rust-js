// The playground itself (ADRs 0032, 0044), as GitHub Pages serves it: its
// static build, in Chromium. It needs rust-js.wasm (`bun run wasm`), the
// compiler it runs; without one, this is skipped.

import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";
import { root, run } from "./support";

const wasm = join(root, "wasm/target/wasm32-wasip1/release/rust-js.wasm");

test.skipIf(!existsSync(wasm))("the playground loads, compiles and runs tests, rendered by React", async () => {
  run(["bun", "run", "site"]);
  const server = Bun.spawn(["bun", "wasm/web/preview.ts"], { cwd: root, env: { ...process.env, PORT: "0" }, stdout: "pipe" });
  const browser = await chromium.launch({ headless: true });
  try {
    // preview.ts prints where it serves the site.
    const reader = server.stdout.getReader();
    let printed = "";
    while (!printed.includes("\n")) printed += new TextDecoder().decode((await reader.read()).value);
    const url = printed.match(/https?:\/\/\S+/)![0];
    const page = await browser.newPage();
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(url);
    // React rendered the page, and the old code found its parts in it.
    await page.locator("#app > h1", { hasText: "rust-js playground" }).waitFor();
    const status = page.locator("#status");
    await status.filter({ hasText: "Ready." }).waitFor({ timeout: 60_000 });
    expect(await page.locator(".cm-editor").count()).toBe(2);
    await page.locator("#test").click();
    await status.filter({ hasText: "Tests: 5 passed, 0 failed." }).waitFor({ timeout: 60_000 });
    await page.locator("#example").selectOption("modules");
    await page.locator("#source-files li", { hasText: "stats.rs" }).waitFor();
    await page.locator("#compile").click();
    await status.filter({ hasText: "Compiled: 5 JS files." }).waitFor({ timeout: 60_000 });
    expect(errors).toEqual([]);
  } finally {
    await browser.close();
    server.kill();
  }
}, 180_000);
