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
| `bless` | write the generated JS as its snapshot, in `test/snapshots/` ([ADR 0050](docs/decisions/0050-snapshots.md)) |
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
The page itself is written in Rust, as React components, and compiled by rust-js: [wasm/web/rust/](wasm/web/rust/components.rs)
([ADRs 0032](docs/decisions/0032-dogfooding-the-playground.md) and [0044](docs/decisions/0044-playground-on-react.md)).

JS is printed by [oxc](https://oxc.rs). The source map points back into the
`.rs` file, so a debugger can show the Rust source.

## Semantics

- Integers wrap on overflow, like Rust's release profile (`overflow-checks = off`).
- Division by zero and `MIN / -1` throw, like Rust in every profile.
- A fieldless enum variant is its name as a string: `Order::Ascending` is `"Ascending"`. One with
  fields is tagged with it, as in ReScript: `Shape::Circle(r)` is `{ TAG: "Circle", _0: r }`.
- An iterator is a JS array, and its adapters the array's methods (`v.iter().map(f)` is `v.map(f)`);
  sorting takes comparators, and `Ordering` is -1, 0 or 1. An `impl Iterator` of the crate's own is a lazy JS
  iterator, `$iterator(it, countdownIterator_next).take(5)` ([ADR 0055](docs/decisions/0055-iterator.md)).
- `thread_local!` is a variable of its module: `const COUNT = { value: 0 };`.
- A JS call whose binding returns a `Result` runs in a `try`: a throw is an `Err`. `?` returns
  an `Err` or a `None` early.
- `Some(x)` is `x` and `None` is `undefined`; a JS `null` counts as `None` too. In generic code,
  a `Some` of a value that would look like `None` is a box, `$some(x)` ([ADR 0051](docs/decisions/0051-generic-options.md)).
- A `const` is the value rustc computed, declared once: `const SIZE = 4096;`.
- Variables are camelCase and taken apart as JS does it: `let (count, set_count) = f();` is
  `const [count, setCount] = f();`.
- A struct is a plain object, `{ x: 1, y: 2 }`; a tuple or tuple struct is an array, `[1, 2]`.
  Rust's copies stay copies: `{ ...a }` where changing one could otherwise be seen through the other.
- An `impl` block's methods are an object named after the type, as Rust's paths name them:
  `Counter::new(1)` is `Counter.new(1)`, and `counter.tick()` is `Counter.tick(counter)`.
- Trait impls have lazy dictionary accessors: `circleShape()`. Concrete calls resolve directly;
  generics receive dictionaries, and `dyn` values are `{ value, impl }`.
- `.clone()` is the value itself unless the two could be told apart: then `{ ...s, tags: s.tags.slice() }`,
  or a call of a hand-written `clone` ([ADR 0052](docs/decisions/0052-std-trait-impls.md)). `==` is `===`,
  or `$eq(a, b)` field by field, and a hand-written `eq` is called wherever it's inside ([ADR 0053](docs/decisions/0053-partial-eq.md)).
- A `Display` impl's `fmt` returns the string it writes: `write!(f, "({}, {})", self.x, self.y)` is
  `return "(" + String(point.x) + ", " + String(point.y) + ")"` ([ADR 0054](docs/decisions/0054-display.md)). `{:?}` is
  written from the type, as Rust shows it: `Point { x: 1.0 }`, `Some(3)`, `(1, "a")` ([ADR 0060](docs/decisions/0060-debug.md)).
- JS is declared in `unsafe extern "Rust"` blocks: `type` for a JS value, `static` for a global,
  `fn` for a function, and a first parameter named `this` for a method.
  `#[link_name = "node:path#join"]` imports from a JS module: `import { join } from "node:path"`.
- A closure is an arrow function; `Rc<Cell<T>>` is one shared `{ value }`; strings are JS strings,
  with JS's methods (`split`, `starts_with`, `replace`, ..), a `char` is a one-character string,
  and `format!` is `+`, with its options: `{:>8.2}` is `$toFixed(x, 2).padStart(8)` ([ADR 0058](docs/decisions/0058-format-options.md)). Byte counts (`len()`, slicing) are errors: JS counts UTF-16 units.
- `async fn` is an `async function` and `.await` is `await`: a future is a JS promise, which starts
  as soon as it's made rather than when first polled.
- React elements are JSX, in a `.jsx` file: `div().class_name("hero").children(title)` is
  `<div className="hero">{title}</div>`.
- Anything not supported yet is reported as a compiler error at the right span.

## Supported so far

`i8`–`i32`, `u8`–`u32`, `f64`, `bool`, enums (with fields too), structs, tuples, `Option`, `const` items; `let`, `if`, `if let`, `while let`, let chains,
`while`, `loop` (with `break value` and labels), `match` on constants, enum variants,
struct and tuple patterns, `_`, bindings, `|` and guards; field reads and writes, indexing arrays, slices and `Vec`s ([ADR 0056](docs/decisions/0056-indexing.md)),
struct update syntax; inherent methods, local traits with defaults and supertraits,
generic functions with explicit dictionaries, read-only trait objects ([ADR 0049](docs/decisions/0049-traits-and-generics.md)),
`Default`, `Clone`, `From`, `PartialEq`, `PartialOrd`/`Ord`, `Display` and `Iterator` impls ([ADRs 0052](docs/decisions/0052-std-trait-impls.md)–[0055](docs/decisions/0055-iterator.md), [0057](docs/decisions/0057-ordering.md)); closures, `&T`, `&mut` to objects, `&str`/`String`, `Box`, `Rc`, `Cell`,
`RefCell`, `Vec`, `HashMap` and `HashSet` with primitive keys ([ADR 0059](docs/decisions/0059-hashmap.md)), `for` loops over sequences and ranges, iterator chains, sorting, `usize`, `to_string()`;
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
`bun run test:react`, `bun run test:browser`, `bun run test:vite`, and
`bun run test:snapshots`, which compares every example's generated JS with
its snapshot in `test/snapshots/`. When a change to the output is intended,
`bun run bless` writes the new JS, and `git diff` shows what changed.
The Vite suite uses a real Chromium browser to check Fast Refresh, dependency
rebuilds, error recovery and JS/JSX extension transitions.

Build tools can pass `--manifest path.json` before `--` to get source
dependencies and final artifact paths. On successful rebuilds, the compiler
removes obsolete artifacts only if that manifest owns them and they have not
been edited. See [the compiler boundaries and manifest contract](docs/decisions/0042-compiler-boundaries-and-build-contract.md).
