# rust-js

Compile Rust to readable JavaScript, in the spirit of ReScript: reuse the
compiler's front end (rustc's parser, type checker and borrow checker), and
replace the back end with one that prints JS from THIR.

```bash
bun run setup                                # once: every package, and the browsers for real-browser tests
bun run build                                # rust-js, and the web and react crates' metadata
./target/debug/rust-js examples/fib.rs       # writes examples/fib.js + fib.js.map
bun run test                                 # native vs. JS, source maps, Rust #[test]s in happy-dom and 3 browsers
```

Every task is a script in [package.json](package.json), and `bun run` lists them:

| Task | What it does |
|---|---|
| `setup` | `bun install` for every package (one [workspace](https://bun.sh/docs/install/workspaces)), and Playwright's browsers |
| `build` | `cargo build`, and the `web` and `react` crates' metadata in `target/` |
| `test` | `bun test` |
| `generate` | regenerate the `web` crate from WebIDL ([web/](web/README.md)) |
| `generate:react` | read each React release into `react/versions.json`, and regenerate `react/src/elements.rs` ([react/](react/README.md)) |
| `react-example` | the [React + Vite example](examples/vite-react/README.md) at http://localhost:5173 |
| `wasm` | build `rust-js.wasm`, with rustc's front end ([wasm/](wasm/README.md)) |
| `dev` | the playground at http://localhost:4400 |
| `site`, `preview` | the playground as static files in `wasm/web/dist`, and serving them as Pages does |
| `deploy` | run the *Deploy playground* workflow |
| `ship` | `wasm`, publish it for the workflow to download, then `deploy` |

**Try it in your browser: https://nguyenyou.github.io/rust-js/**. That page runs rustc's front end
and rust-js as WebAssembly, so nothing is compiled on a server (see [wasm/](wasm/README.md)).
The page itself is written in Rust and compiled by rust-js: [wasm/web/rust/lib.rs](wasm/web/rust/lib.rs)
([ADR 0032](docs/decisions/0032-dogfooding-the-playground.md)).

JS is printed by [oxc](https://oxc.rs). The source map points back into the
`.rs` file, so a debugger can show the Rust source.

## Semantics

- Integers wrap on overflow, like Rust's release profile (`overflow-checks = off`).
- Division by zero and `MIN / -1` throw, like Rust in every profile.
- A fieldless enum variant is its name as a string: `Order::Ascending` is `"Ascending"`. One with
  fields is tagged with it, as in ReScript: `Shape::Circle(r)` is `{ TAG: "Circle", _0: r }`.
- An iterator is a JS array, and its adapters the array's methods (`v.iter().map(f)` is `v.map(f)`);
  sorting takes comparators, and `Ordering` is -1, 0 or 1.
- `thread_local!` is a variable of its module: `const COUNT = { value: 0 };`.
- A JS call whose binding returns a `Result` runs in a `try`: a throw is an `Err`. `?` returns
  an `Err` or a `None` early.
- `Some(x)` is `x` and `None` is `undefined`; a JS `null` counts as `None` too.
- A `const` is the value rustc computed, declared once: `const SIZE = 4096;`.
- Variables are camelCase and taken apart as JS does it: `let (count, set_count) = f();` is
  `const [count, setCount] = f();`.
- A struct is a plain object, `{ x: 1, y: 2 }`; a tuple or tuple struct is an array, `[1, 2]`.
  Rust's copies stay copies: `{ ...a }` where changing one could otherwise be seen through the other.
- JS is declared in `unsafe extern "Rust"` blocks: `type` for a JS value, `static` for a global,
  `fn` for a function, and a first parameter named `this` for a method.
  `#[link_name = "node:path#join"]` imports from a JS module: `import { join } from "node:path"`.
- A closure is an arrow function; `Rc<Cell<T>>` is one shared `{ value }`; strings are JS strings,
  with JS's methods (`split`, `starts_with`, `replace`, ..), a `char` is a one-character string,
  and `format!` is `+`. Byte counts (`len()`, slicing) are errors: JS counts UTF-16 units.
- `async fn` is an `async function` and `.await` is `await`: a future is a JS promise, which starts
  as soon as it's made rather than when first polled.
- React elements are JSX, in a `.jsx` file: `div().class_name("hero").children(title)` is
  `<div className="hero">{title}</div>`.
- Anything not supported yet is reported as a compiler error at the right span.

## Supported so far

`i8`–`i32`, `u8`–`u32`, `f64`, `bool`, enums (with fields too), structs, tuples, `Option`, `const` items; `let`, `if`, `if let`, `while let`,
`while`, `loop` (with `break value` and labels), `match` on constants, enum variants,
struct and tuple patterns, `_`, bindings, `|` and guards; field reads and writes,
struct update syntax; closures, `&T`, `&mut` to objects, `&str`/`String`, `Box`, `Rc`, `Cell`,
`RefCell`, `Vec`, `for` loops over sequences and ranges, iterator chains, sorting, `usize`, `to_string()`;
JS functions, methods and globals, generic bindings, and imports from JS modules; `async`/`.await`;
calls between functions, across modules and files, and functions as values; React components, as JSX.

The DOM comes as the [`web`](web/README.md) crate: bindings generated from W3C's WebIDL
([ADR 0024](docs/decisions/0024-web-crate.md)). [examples/counter.rs](examples/counter.rs) is a
counter written with it, [examples/todo.rs](examples/todo.rs) a todo list, and
[examples/countdown.rs](examples/countdown.rs) a countdown with `async` code, and
[examples/fetch.rs](examples/fetch.rs) a `fetch`. All run in the
playground's Result pane:

```bash
bun run build
./target/debug/rust-js examples/counter.rs -- --extern web=target/libweb.rmeta
```

## React

The [`react`](react/README.md) crate binds React, and a component compiles to the JSX you'd write
by hand ([ADR 0041](docs/decisions/0041-react.md)):

```rust
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button().on_click(move |_| set_count.update(|count| count + 1)).children(("Count is ", count))
}
```

```jsx
export function App() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount((count) => count + 1 | 0)}>Count is {count}</button>;
}
```

[examples/vite-react](examples/vite-react/README.md) is create-vite's React template with `App.jsx`
written in Rust. [vite-plugin-rust-js](vite-plugin/index.js) compiles it on every save, and Fast
Refresh keeps the page's state: `bun run react-example`. It uses React Compiler and Tailwind CSS
(which reads its classes in the `.rs` file), each set up as its own guide does.

## Modules

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
bun run setup                                       # happy-dom, for DOM tests
./target/debug/rust-js --test examples/todo.rs -o out/todo.js -- --extern web=target/libweb.rmeta
bun test --preload ./test/happydom.ts ./out/todo.test.js
```

`assert!`, `assert_eq!`, `panic!` and `#[should_panic]` fail with Rust's messages.

The same tests run in Chromium with Playwright Test or Vitest's
browser mode, both on Bun ([browser/](browser/README.md), [ADR 0027](docs/decisions/0027-real-browser-tests.md)).
A test that needs one is marked `#[cfg_attr(not(browser), ignore)]`.

Design decisions are recorded in [docs/](docs/README.md).

The suites can also run independently: `bun run test:compiler`,
`bun run test:react`, `bun run test:browser`, and `bun run test:vite`.
The Vite suite uses a real Chromium browser to check Fast Refresh, dependency
rebuilds, error recovery and JS/JSX extension transitions.

Build tools can pass `--manifest path.json` before `--` to get source
dependencies and final artifact paths. On successful rebuilds, the compiler
removes obsolete artifacts only if that manifest owns them and they have not
been edited. See [the compiler boundaries and manifest contract](docs/decisions/0042-compiler-boundaries-and-build-contract.md).
