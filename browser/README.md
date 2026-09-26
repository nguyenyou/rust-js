# Tests in real browsers

rust-js compiles a crate's `#[test]` functions with `rust-js --test`, into a
`.test.js` file that calls `test()` for each one (ADR 0026). `bun test` runs
it in happy-dom; this folder runs the same file in **Chromium**, through
Playwright, on Bun. See
[ADR 0027](../docs/decisions/0027-real-browser-tests.md).

```bash
bun run setup                                  # once: the tools, and the browsers

# Compile the tests for a browser: `--cfg browser` turns on the ones that need one.
./target/debug/rust-js --test examples/counter.rs -o out/counter.js -- --extern web=target/libweb.rmeta --cfg=browser

# Then either runner:
RUST_JS_TESTS=out/counter.test.js bunx --bun playwright test -c browser/playwright.config.ts
RUST_JS_TESTS=out/counter.test.js bunx --bun vitest run -c browser/vitest.config.ts
```

`RUST_JS_TESTS` takes several files, separated by spaces.

| File | What it is |
|---|---|
| `playwright.config.ts` | Playwright Test: Chromium |
| `rust-tests.spec.ts` | Runs each Rust test in a fresh page |
| `vitest.config.ts` | Vitest's browser mode, with the Playwright provider |

Mark a test that only makes sense in a real browser, such as one that needs
layout, which happy-dom doesn't do:

```rust
#[test]
#[cfg_attr(not(browser), ignore = "needs a real browser")]
fn the_count_sits_between_the_buttons() { .. }
```
