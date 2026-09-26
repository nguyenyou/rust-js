import { beforeAll, expect, test } from "bun:test";
import { join } from "node:path";
import { stripVTControlCharacters } from "node:util";
import { root, target, run, buildCompiler, buildWeb } from "./support";

beforeAll(() => {
  buildCompiler();
  buildWeb();
  const withWeb = ["--", "--extern", `web=${join(target, "libweb.rmeta")}`];
  // Test mode (ADR 0026): the same programs with their `#[test]`s, and some failing on purpose.
  const tests = (rs: string, name: string, flags: string[] = []) =>
    run([join(target, "debug", "rust-js"), "--test", rs, "-o", join(target, "rust-tests", name, `${name}.js`), ...flags]);
  tests("examples/counter.rs", "counter", withWeb);
  tests("examples/todo.rs", "todo", withWeb);
  tests("examples/countdown.rs", "countdown", withWeb);
  tests("test/asserts.rs", "asserts");
  // For real browsers (ADR 0027): `--cfg browser` turns on tests that need one.
  const forBrowser = (rs: string, name: string, flags: string[] = []) =>
    run([join(target, "debug", "rust-js"), "--test", rs, "-o", join(target, "browser-tests", name, `${name}.js`), ...flags, "--cfg=browser"]);
  forBrowser("examples/counter.rs", "counter", withWeb);
  forBrowser("examples/todo.rs", "todo", withWeb);
  forBrowser("test/asserts.rs", "asserts", ["--"]);
}, 600_000);

// ADR 0026: `#[test]` functions, in Rust, compiled by `rust-js --test` and
// run by `bun test` in happy-dom's DOM.
function rustTests(files: string[]): { exit: number; output: string } {
  const p = Bun.spawnSync(["bun", "test", "--preload", "./test/happydom.ts", ...files.map((f) => `./${f}`)], {
    cwd: root,
    stderr: "pipe",
  });
  return { exit: p.exitCode ?? -1, output: p.stdout.toString() + p.stderr.toString() };
}

test("the example apps' own tests pass, in a DOM", () => {
  const apps = ["counter", "todo", "countdown"].map((app) => `target/rust-tests/${app}/${app}.test.js`);
  const { exit, output } = rustTests(apps);
  expect([exit, output.match(/(\d+) pass/)?.[1], output.match(/(\d+) fail/)?.[1]]).toEqual([0, "8", "0"]);
});

test("a failing test fails the way Rust's would", () => {
  const { exit, output } = rustTests(["target/rust-tests/asserts/asserts.test.js"]);
  expect(exit).toBe(1);
  expect([output.match(/(\d+) pass/)?.[1], output.match(/(\d+) skip/)?.[1], output.match(/(\d+) fail/)?.[1]]).toEqual(["3", "1", "4"]);
  // `assert!` with a message; `assert_eq!` showing both sides, as Rust does.
  expect(output).toContain("error: n was 3");
  expect(output).toContain("error: assertion `left == right` failed\n  left: { x: 1, y: 2 }\n right: { x: 1, y: 3 }");
  // `#[should_panic]`: the wrong message, and no panic at all.
  expect(output).toContain('panic message: "\\"something\\" happened"\n expected substring: "nope"');
  expect(output).toContain("error: test did not panic as expected");
  for (const name of ["fails_an_assert", "fails_an_assert_eq", "panics_with_the_wrong_message", "does_not_panic"]) {
    expect(output).toContain(`(fail) tests::${name}`);
  }
});

// ADR 0027: the same tests in Chromium,
// through Playwright Test and through Vitest's browser mode, both on Bun.
const browserTests = ["target/browser-tests/counter/counter.test.js", "target/browser-tests/todo/todo.test.js"];

function inBrowsers(runner: "playwright" | "vitest", files: string[]): { exit: number; output: string } {
  const command =
    runner === "playwright"
      ? ["bunx", "--bun", "playwright", "test", "-c", "browser/playwright.config.ts", "--reporter=line"]
      : ["bunx", "--bun", "vitest", "run", "-c", "browser/vitest.config.ts"];
  const p = Bun.spawnSync(command, { cwd: root, env: { ...process.env, RUST_JS_TESTS: files.join(" ") }, stderr: "pipe" });
  // CI reporters may color individual words and numbers in their summaries.
  return { exit: p.exitCode ?? -1, output: stripVTControlCharacters(p.stdout.toString() + p.stderr.toString()) };
}

test("in real browsers, with Playwright Test on Bun", () => {
  const { exit, output } = inBrowsers("playwright", browserTests);
  // 8 tests, the layout one included, on Chromium.
  expect([exit, output.match(/(\d+) passed/)?.[1]], output).toEqual([0, "8"]);
  const failing = inBrowsers("playwright", ["target/browser-tests/asserts/asserts.test.js"]);
  expect([failing.exit, failing.output.match(/(\d+) failed/)?.[1], failing.output.match(/(\d+) skipped/)?.[1]]).toEqual([1, "4", "1"]);
  expect(failing.output).toContain("Error: assertion `left == right` failed\n      left: { x: 1, y: 2 }");
}, 120_000);

test("in real browsers, with Vitest's browser mode", () => {
  const { exit, output } = inBrowsers("vitest", browserTests);
  expect([exit, output.match(/Tests\s+(\d+) passed/)?.[1]], output).toEqual([0, "8"]);
  const failing = inBrowsers("vitest", ["target/browser-tests/asserts/asserts.test.js"]);
  expect([failing.exit, failing.output.match(/Tests\s+(\d+) failed/)?.[1]]).toEqual([1, "4"]);
  // Vitest follows the source map back into the Rust.
  expect(failing.output).toContain("fails_an_assert test/asserts.rs:");
}, 120_000);
