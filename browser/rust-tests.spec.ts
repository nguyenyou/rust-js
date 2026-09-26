// rust-js's compiled `#[test]`s, run in real browsers by Playwright Test
// (ADR 0027). Playwright runs on Bun:
//
//   RUST_JS_TESTS=out/app.test.js bunx --bun playwright test -c browser/playwright.config.ts
//
// Each Rust test gets a fresh page, which loads the compiled test file as an
// ES module next to a global `test()` that only records, then runs that one
// test. The page is served from a made-up origin by `page.route`: no server.

import { readFileSync } from "node:fs";
import { basename, dirname, relative, resolve, sep } from "node:path";

import { test } from "@playwright/test";

const ORIGIN = "http://rust-js.test";

const files = (process.env.RUST_JS_TESTS ?? "").split(/[\s,]+/).filter(Boolean);
if (files.length === 0) {
  throw new Error("set RUST_JS_TESTS to the .test.js files that `rust-js --test` wrote");
}

/** The page: `test()` as bun test and Vitest provide it, and a way to run one. */
const page = (testFile: string) => `<!doctype html>
<meta charset="utf-8">
<body></body>
<script type="module">
  const tests = new Map();
  globalThis.test = (name, f) => tests.set(name, f);
  test.skip = () => {};
  const loaded = import(${JSON.stringify(`./${testFile}`)});
  // A panic in an event handler doesn't reach the test: the browser reports it.
  const escaped = [];
  addEventListener("error", (e) => escaped.push(e.message));
  window.runRustTest = async (name) => {
    await loaded;
    try {
      tests.get(name)();
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
    return escaped.length ? escaped.join("\\n") : null;
  };
</script>`;

for (const file of files) {
  const path = resolve(file);
  const dir = dirname(path);
  // The tests' names, as \`rust-js --test\` wrote them: test("tests::adds", ..).
  const source = readFileSync(path, "utf8");
  const found = [...source.matchAll(/^(test|test\.skip)\(("(?:[^"\\]|\\.)*"), /gm)];

  test.describe(relative(process.cwd(), path), () => {
    for (const [, kind, label] of found) {
      const name: string = JSON.parse(label);
      (kind === "test.skip" ? test.skip : test)(name, async ({ page: tab }) => {
        await tab.route(`${ORIGIN}/**`, (route) => {
          const url = new URL(route.request().url());
          if (url.pathname === "/") {
            return route.fulfill({ contentType: "text/html", body: page(basename(path)) });
          }
          // Only files next to the test file.
          const target = resolve(dir, `.${decodeURIComponent(url.pathname)}`);
          return target.startsWith(dir + sep) ? route.fulfill({ path: target }) : route.fulfill({ status: 404 });
        });
        await tab.goto(`${ORIGIN}/`);
        const failure = await tab.evaluate(`window.runRustTest(${JSON.stringify(name)})`);
        if (failure !== null) throw new Error(failure);
      });
    }
  });
}
