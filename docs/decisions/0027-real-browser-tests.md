# 0027. Real-browser tests: Playwright Test and Vitest's browser mode, on Bun

Status: Accepted. Extends [0026](0026-testing.md).

## Context

happy-dom (ADR 0026) is fast, but it's a simulation. It does no layout
(every `getBoundingClientRect()` is zeros), and some APIs and behaviors are
only in real engines. Some tests need a real browser. How the others do it
(checked in local clones):

- **Scala.js** swaps the whole *environment* and keeps the tests:
  `Test / jsEnv := new PWEnv(browserName = "chrome")`
  (gmkumar2005/scala-js-env-playwright), or the official `SeleniumJSEnv`.
  The environment opens a page, injects a `setup.js` that captures
  `console` and adds a message channel, and loads the compiled suite. **The
  test framework runs inside the browser**; sbt reads the results back.
- **ReScript**'s own website runs its tests in Chromium with **Vitest's
  browser mode and the Playwright provider** (`@vitest/browser-playwright`),
  through 32 hand-written `external` bindings to `test`, `expect` and
  `render`. Again, the tests run inside the page.

Bun 1.4 runs Playwright: `bunx --bun playwright test` with a
`playwright.config.ts`. Bun also has its own `Bun.WebView`, a headless
browser in the runtime (experimental).

## Decision

**The same compiled tests run in real browsers, two ways**, both on Bun,
both with Playwright's Chromium, Firefox and WebKit:

```bash
RUST_JS_TESTS=out/app.test.js bunx --bun playwright test -c browser/playwright.config.ts
RUST_JS_TESTS=out/app.test.js bunx --bun vitest run -c browser/vitest.config.ts
```

- **Playwright Test** (`browser/rust-tests.spec.ts`) reads each test's name
  from the `.test.js` file and gives every Rust test **a fresh page**. The
  page loads the compiled file as an ES module, next to a global `test()`
  that only records, and runs that one test. The files are served from a
  made-up origin by `page.route`, so there's no server to start. A panic in
  an event handler, which a browser only reports, fails the test too. This
  is Scala.js's `PWEnv` model, with Playwright's reporters, `--ui`, traces
  and parallel workers.
- **Vitest's browser mode** (`browser/vitest.config.ts`) runs each test
  file inside the page. Its `globals` provide the `test()` that the
  generated files call, so they need no changes. It follows the source maps
  back into the `.rs` file, and takes a screenshot of each failure.

**Tests that need a real browser say so in Rust**, with a cfg:

```rust
#[test]
#[cfg_attr(not(browser), ignore = "needs a real browser")]
fn the_count_sits_between_the_buttons() { .. }
```

Browser runs compile with `-- --cfg browser`, and happy-dom runs without it.
rust-js declares the cfg to rustc (`--check-cfg=cfg(browser, test)`), so
neither case warns. The playground's Test button runs in a real browser,
so it passes `--cfg browser` too.

## Why

- **One test source, three places to run it**: happy-dom for speed, and
  real engines when it matters. The generated file only calls `test()`,
  which is the one thing all three runners provide.
- **Two familiar runners**, as asked: Playwright Test for Playwright users,
  and Vitest for those who know it, which is what the ReScript website uses.
- **All on Bun**: no Node, and Playwright's browsers are the only download.
- **Three engines**: a test that passes in Chromium can still fail in WebKit.

## Alternatives

- **`Bun.WebView`**: needs no browser download, and its input is trusted.
  But it's experimental, has one engine per platform (WebKit on macOS), and
  would need a runner and reporter of its own. Worth revisiting once it's
  stable.
- **Driving the page from outside** (Playwright Test's usual style, with
  test code in Node or Bun): the tests are Rust compiled to *page* code,
  so they belong inside the page, as in Scala.js and Vitest.
- **Only one of the two runners**: either would do, but both are small, and
  people come with one or the other.

## Consequences

- The repo's `package.json` has `"type": "module"`. Without it, Playwright
  on Bun loads a `.ts` spec as CommonJS, and fails on its TypeScript.
- Browsers come from `bunx playwright install` (Chromium, Firefox, WebKit).
- rust-js's own suite runs the counter and the todo app in all three
  engines through both runners, 24 tests each, and `test/asserts.rs`
  there too for the failure messages. It takes about 17 seconds now.
- Rust tests still create input with DOM calls (`click()`, `dispatchEvent`),
  not trusted input: that would need async tests. Playwright's and Vitest's
  own tests can use trusted input next to them.
