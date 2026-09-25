# 0026. Tests are Rust's `#[test]`, run by `bun test` in happy-dom

Status: Accepted. Extends [0012](0012-panics-and-runtime-helpers.md) to
`panic!` and the assertion macros.

## Context

A program written with rust-js needs tests. How the others do it (checked
in the local clones):

- **Scala.js (Laminar):** tests are Scala (ScalaTest's `it(..)`), compiled
  by Scala.js and run in Node with jsdom as the browser (`JSDOMNodeJSEnv`
  in `build.sbt`). `domtestutils` adds `mount(..)` and `expectNode(..)`.
- **ReScript** has no test framework of its own. A test is a `.res` file
  calling a JS runner through `external` bindings: the compiler's tests
  bind Mocha's `describe` and `test` in a two-line `mocha.res`, and Node's
  `assert` for checks. `@rescript/webapi` mostly compiles its tests and
  diffs the JS with `git`; the few it runs, run in plain Node.

rust-js had only tests of rust-js itself: native Rust against the JS it
generates, and a hand-written fake DOM in TypeScript for the counter and
the todo app. Nobody could test a rust-js program in Rust.

## Decision

**Tests are Rust's own `#[test]` functions**, next to the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_adds_a_trimmed_title() {
        let app = page();
        type_in(app, "  Buy milk  ", "Enter");
        assert_eq!(titles(app), ["Buy milk"]);
    }
}
```

**`rust-js --test app.rs -o out/app.js`** compiles with rustc's `--test`. So
rustc parses `#[test]`, `#[should_panic]` and `#[ignore]`, and adds its
harness: a `const` per test, marked `#[rustc_test_marker = "tests::adds"]`,
and a `main` that runs them with libtest. rust-js leaves the harness out,
compiles the test functions like any others, exports them, and writes
`out/app.test.js`:

```js
import * as tests from "./tests.js";

test("tests::adds", tests.adds);
test("tests::divides_by_zero", () => shouldPanic(tests.divides_by_zero, "divide by zero"));
test.skip("tests::slow", tests.slow);
```

`test` is the runner's global. **`bun test` runs it**, with **happy-dom** as
the page, installed as globals by a preload
(`@happy-dom/global-registrator`):

```bash
bun test --preload ./test/happydom.ts out/app.test.js
```

**Panics and assertions** lower to what Rust would print:

| Rust | JS |
|---|---|
| `panic!("negative")`, `unreachable!()`, `todo!()` | `throw new Error("negative")` |
| `assert!(n < 2)` | `throw new Error("assertion failed: n < 2")` |
| `panic!("n was {} and {:?}", n, s)` | `"n was " + args[0] + " and " + args[1]`, built from the args |
| `assert_eq!(a, b)`, `assert_ne!` | `$assertFailed("Eq", a, b)`: Rust's message, with both sides |
| `==` on structs, tuples, arrays, `Vec`s | `$eq(a, b)`, field by field |
| `#[should_panic(expected = "..")]` | passes only on a panic whose message contains it |

`format_args!` is decoded at compile time from the template rustc builds
(its encoding is documented in core's `fmt::Arguments`). `{}` takes strings,
integers and `bool`; `{:?}` takes anything, through a small `$debug` helper.

**Tests set up their own page:** happy-dom keeps one `document` per test
file, so each test empties `document.body` and adds the elements it needs.

## Why

- **It's what Rust programmers write.** `#[test]`, `assert_eq!` and
  `#[should_panic]` mean what they always mean. rustc does the parsing, so
  rust-js only adds what JS needs.
- **The runner is off the shelf.** `bun test` gives discovery, reporting,
  `--watch` and filters. The generated file only calls `test(..)`, which
  other runners with globals (Jest, Vitest) have too.
- **happy-dom is a real enough DOM:** `click()` on a checkbox toggles it and
  fires `change`, as the todo tests rely on. bun's documentation recommends
  it, and it starts fast.
- **Failures read like Rust's**, down to `left:` and `right:`.

## Alternatives

- **Bindings to the runner, as ReScript does** (`test`, `expect` in an
  `extern` block): possible, but `expect` takes any JS value, which rust-js
  can't type yet. And Rust programmers expect `#[test]`.
- **jsdom, as Laminar uses:** more complete, but heavier and slower to
  start. happy-dom covers what these programs do.
- **Compile-only tests with committed JS**, as `@rescript/webapi` does:
  useful for bindings, but they don't run anything.
- **rust-js's own runner:** reporting, filtering and watching are what test
  runners already do well.

## Consequences

- rust-js's own tests use this for the counter and the todo app. The
  hand-written fake DOM is gone. `test/asserts.rs` fails on purpose, to
  pin down every failure message.
- `bunfig.toml` limits discovery to `test/`, so a plain `bun test` doesn't
  pick up the generated `*.test.js` files by itself.
- `{:?}` of a struct prints `{ x: 1, y: 2 }`, not `Point { x: 1, y: 2 }`:
  its type name isn't in the JS.
- Not yet: formatting options (`{:>8}`, `{:.2}`, `{:#?}`), `{}` of floats or
  of types with their own `Display`, tests returning `Result`, and
  per-test timeouts.
- Panics now work everywhere, not only in tests: `unreachable!()` in a
  `match` arm is a `throw`.
