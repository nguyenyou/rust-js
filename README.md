# rust-js

Compile Rust to readable JavaScript, in the spirit of ReScript: reuse the
compiler's front end (rustc's parser, type checker and borrow checker), and
replace the back end with one that prints JS from THIR.

```bash
cargo build
./target/debug/rust-js examples/fib.rs          # writes examples/fib.js + fib.js.map
bun install && bunx playwright install      # test tools, and the browsers for real-browser tests
bun test                                     # native vs. JS, source maps, Rust #[test]s in happy-dom and 3 browsers
```

**Try it in your browser: https://nguyenyou.github.io/rust-js/**. That page runs rustc's front end
and rust-js as WebAssembly, so nothing is compiled on a server (see [wasm/](wasm/README.md)).

JS is printed by [oxc](https://oxc.rs). The source map points back into the
`.rs` file, so a debugger can show the Rust source.

## Semantics

- Integers wrap on overflow, like Rust's release profile (`overflow-checks = off`).
- Division by zero and `MIN / -1` throw, like Rust in every profile.
- A fieldless enum variant is its name as a string: `Order::Ascending` is `"Ascending"`.
- A struct is a plain object, `{ x: 1, y: 2 }`; a tuple or tuple struct is an array, `[1, 2]`.
  Rust's copies stay copies: `{ ...a }` where changing one could otherwise be seen through the other.
- JS is declared in `unsafe extern "Rust"` blocks: `type` for a JS value, `static` for a global,
  `fn` for a function, and a first parameter named `this` for a method.
- A closure is an arrow function; `Rc<Cell<T>>` is one shared `{ value }`; strings are JS strings.
- Anything not supported yet is reported as a compiler error at the right span.

## Supported so far

`i8`–`i32`, `u8`–`u32`, `f64`, `bool`, fieldless enums, structs, tuples; `let`, `if`,
`while`, `loop` (with `break value` and labels), `match` on constants, enum variants,
struct and tuple patterns, `_`, bindings, `|` and guards; field reads and writes,
struct update syntax; closures, `&T`, `&mut` to objects, `&str`/`String`, `Box`, `Rc`, `Cell`,
`RefCell`, `Vec`, `for` loops over sequences and ranges, `usize`, `to_string()`;
JS functions, methods and globals; calls between functions, across modules and files.

The DOM comes as the [`web`](web/README.md) crate: bindings generated from W3C's WebIDL
([ADR 0024](docs/decisions/0024-web-crate.md)). [examples/counter.rs](examples/counter.rs) is a
counter written with it, and [examples/todo.rs](examples/todo.rs) a todo list. Both run in the
playground's Result pane:

```bash
web/build.sh -o target/libweb.rmeta
./target/debug/rust-js examples/counter.rs -- --extern web=target/libweb.rmeta
```

A crate split across files becomes one JS file per module, with the imports
and exports written for you (see [ADR 0019](docs/decisions/0019-one-js-file-per-module.md)):

```bash
./target/debug/rust-js examples/modules/lib.rs -o out/lib.js   # writes out/lib.js, out/stats.js, ...
```

## Testing

Tests are Rust's own `#[test]` functions ([ADR 0026](docs/decisions/0026-testing.md)).
`rust-js --test` compiles them, and writes a `.test.js` file for `bun test`, which runs them
in happy-dom's DOM:

```bash
bun install                                         # happy-dom, for DOM tests
./target/debug/rust-js --test examples/todo.rs -o out/todo.js -- --extern web=target/libweb.rmeta
bun test --preload ./test/happydom.ts ./out/todo.test.js
```

`assert!`, `assert_eq!`, `panic!` and `#[should_panic]` fail with Rust's messages.

The same tests run in real browsers (Chromium, Firefox, WebKit) with Playwright Test or Vitest's
browser mode, both on Bun ([browser/](browser/README.md), [ADR 0027](docs/decisions/0027-real-browser-tests.md)).
A test that needs one is marked `#[cfg_attr(not(browser), ignore)]`.

Design decisions are recorded in [docs/](docs/README.md).
