# rust-js

**Rust in. Readable JavaScript out.**

rust-js brings Rust's type system and borrow checker to JavaScript. Inspired
by ReScript, it keeps rustc's front end and replaces code generation with a
JavaScript back end. The goal is output you can read, debug, and call from
JavaScript as if you had written it yourself.

[Try the playground](https://nguyenyou.github.io/rust-js/) — compilation runs
entirely in your browser. The playground itself is written in Rust and
compiled by rust-js.

## The idea

- **Keep Rust's checks.** rustc checks types, traits, ownership, and lifetimes
  before any JavaScript is emitted.
- **Use JavaScript's building blocks.** Structs become objects, closures become
  arrow functions, and React components become JSX. Runtime helpers are emitted
  only where needed.
- **Fit the tools you already use.** ES modules, JavaScript imports, DOM bindings,
  React, and Vite with Fast Refresh. Source maps point back to your Rust.
- **Make the choices explicit.** Support grows one feature at a time. Unsupported
  features produce compiler errors; differences from native Rust are
  [documented](docs/README.md).

For example, a React component:

```rust
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button()
        .on_click(move |_| set_count.update(|count| count + 1))
        .children(("Count is ", count))
}
```

becomes:

```jsx
export function App() {
  const [count, setCount] = useState(0);
  return (
    <button onClick={() => setCount((count) => (count + 1) | 0)}>
      Count is {count}
    </button>
  );
}
```

Imports omitted; see the [React guide](react/README.md) for a complete example.

## Get started

Install Bun and Rust (via rustup), then run these from the repository root.
The repository pins the required Rust nightly and components.

```bash
bun run setup
bun run build
./target/debug/rust-js examples/fib.rs  # writes fib.js and fib.js.map beside fib.rs
bun run react-example                 # React + Vite at localhost:5173
```

Run `bun test` for native Rust comparisons, output snapshots, and Chromium tests.
Run `bun run` to list all tasks.

rust-js is a growing subset of Rust with JavaScript representations and
execution rules. For example, async functions produce eager JavaScript promises.
See the [design decisions](docs/README.md) for supported behavior and tradeoffs.

[Examples](examples/) · [React + Vite](examples/vite-react/README.md) ·
[DOM bindings](web/README.md) · [Browser tests](browser/README.md) ·
[Compiler in WebAssembly](wasm/README.md)
