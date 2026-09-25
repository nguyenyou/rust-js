# rsjs

Compile Rust to readable JavaScript, in the spirit of ReScript: reuse the
compiler's front end (rustc's parser, type checker and borrow checker), and
replace the back end with one that prints JS from THIR.

```bash
cargo build
./target/debug/rsjs examples/fib.rs          # writes examples/fib.js + fib.js.map
bun test                                     # native Rust vs. generated JS, and the source map
```

JS is printed by [oxc](https://oxc.rs). The source map points back into the
`.rs` file, so a debugger can show the Rust source.

## Semantics

- Integers wrap on overflow, like Rust's release profile (`overflow-checks = off`).
- Division by zero and `MIN / -1` throw, like Rust in every profile.
- A fieldless enum variant is its name as a string: `Order::Ascending` is `"Ascending"`.
- Anything not supported yet is reported as a compiler error at the right span.

## Supported so far

`i8`–`i32`, `u8`–`u32`, `f64`, `bool`, fieldless enums; `let`, `if`, `while`,
`loop` (with `break value` and labels), `match` on constants, enum variants,
`_`, bindings, `|` and guards; calls between top-level functions.

Design decisions are recorded in [docs/](docs/README.md).
