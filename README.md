# rust-js

**Rust in. Readable JavaScript out.**

rust-js compiles Rust to readable JavaScript. Built on rustc and inspired by
ReScript, it keeps Rust's type system, traits, ownership, borrow checking,
and compiler diagnostics while replacing code generation with a JavaScript
backend.

The goal is simple:

> **Write Rust. Get JavaScript you would have been willing to write by hand.**

[Try the playground](https://rust-js-lang.github.io/rust-js/) — compilation runs
entirely in your browser. The playground itself is written in Rust and
compiled by rust-js.

## Why Rust to JavaScript?

Rust gives us a powerful way to describe programs and catch mistakes before
they run. JavaScript gives us the runtime and ecosystem in which much of our
software already lives: browsers, React, npm packages, and tools such as Vite
and Bun.

We want to use them together. Write a React component in Rust, import a
JavaScript package, call a browser API, and inspect the result in DevTools.
Let Rust check the program and let JavaScript run it.

WebAssembly provides a way to run compiled Rust in the browser. rust-js explores
a different compilation target: JavaScript itself. Its output is ordinary ES
modules and JSX that existing JavaScript applications can import, debug, and
build with their usual tools.

The longer-term motivation is Rust across the application stack. A server can
use native Rust while a frontend uses rust-js, with shared types and data
models describing the contract between them. Support for
`#[derive(Serialize, Deserialize)]` is one step toward that: for supported
types and `#[serde(...)]` attributes, generated code writes and reads JSON
with serde_json's behavior, including its deserialization errors.

## What that looks like

A React component written in Rust:

```rust
pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    jsx! {
        <button onClick={move |_| set_count.update(|count| count + 1)}>
            {"Count is "}{count}
        </button>
    }
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

The component remains a function, state remains a React hook, and the event
handler remains an arrow function. The `| 0` keeps the addition within a signed
32-bit integer, following rust-js's release-style integer arithmetic.

## Philosophy

### Keep Rust's compiler in charge

rustc handles parsing, macro expansion, name resolution, types, traits,
ownership, lifetimes, and borrow checking. rust-js uses its checked program
representation to generate JavaScript.

A program must pass Rust's checks before rust-js emits output. Passing those
checks is necessary, but the program must also use constructs that rust-js
supports. Unsupported constructs produce compiler errors.

### Generated code is part of the product

You should be able to open the generated file, understand it, debug it, and
call it from JavaScript. Names, module structure, formatting, and source maps
all matter because people will work with this code.

That starts with representations a JavaScript programmer would recognize:

| Rust | JavaScript |
| --- | --- |
| `struct` | object |
| tuple | array |
| closure | arrow function |
| `Vec<T>` | `Array` |
| `HashMap<K, V>` | `Map`, for supported key types |
| `HashSet<T>` | `Set`, for supported element types |
| `Option<T>` | usually a value or `undefined`; generic code boxes ambiguous values |
| `async` / `.await` | `async` / `await`, with promises |
| module | ES module |

Runtime helpers are emitted where the required behavior needs them. Each
helper should earn its place; readable output is a design constraint throughout
the compiler.

### Work with JavaScript's runtime

Rust and JavaScript have different runtime models. Reproducing every detail of
native Rust would require machinery that can obscure the generated program.
rust-js favors JavaScript's own building blocks and makes the resulting
semantic choices explicit.

For example, calling an async function starts its work immediately, up to the
first `await`, just as it does in JavaScript. Native Rust futures wait until
they are polled. This choice lets JavaScript callers receive an ordinary
promise and lets the JavaScript event loop run the work without a Rust executor.
It also means a future that is never awaited can still have effects.

These differences belong in the design, documentation, and tests. The
[design decisions](docs/README.md) record the behavior, its reasons, and its
costs. When a feature has no supported translation, compilation should fail
clearly.

### Make interop fundamental

Importing JavaScript packages, exporting functions, using DOM APIs, and working
with existing bundlers are core concerns. A Rust component should fit into a
React application, and its output should remain useful to someone writing
JavaScript.

### Test the promise in real programs

Where behavior should match native Rust, run the same program through both
implementations and compare the results. Snapshot the generated JavaScript
so changes to its readability can be reviewed too.

Real applications test how those pieces fit together. The playground is
written in Rust, compiled by rust-js, and built with React and Vite. Developing
it exercises the same compiler, bindings, and tooling that other applications
use.

## What works today

rust-js supports a growing subset of Rust aimed at JavaScript applications:

- structs, tuples, enums, pattern matching, and methods
- traits and generics within the [supported subset](docs/decisions/0049-traits-and-generics.md)
- closures, iterators, `Option`, `Result`, and common collection operations
- strings, formatting, and `async` / `await`
- ES modules, JavaScript imports, and DOM bindings generated from WebIDL
- React, JSX, Vite, Fast Refresh, and source maps
- serde-compatible JSON writing and reading (see the
  [design decisions](docs/README.md))

Support is specific to each feature. For example, generic trait methods remain
unsupported, map keys must have a supported value-based representation, and
JSON deserialization does not yet handle borrowed strings or `#[serde(flatten)]`.
The linked design decisions describe the current boundaries.

## Get started

Install [Bun](https://bun.sh/) and [Rust](https://rustup.rs/), then run these
commands from the repository root.
The repository pins the required Rust nightly and components.

```bash
bun run setup
bun run build
./target/debug/rust-js examples/fib.rs  # writes fib.js and fib.js.map beside fib.rs
bun run react-example                 # React + Vite at localhost:5173
```

## Testing

Run `bun test` for native Rust comparisons, output snapshots, compiler
diagnostics, source maps, React, browser, and Vite integration tests.
Run `bun run fmt` to format Rust and playground JSX, or `bun run fmt:check` to check.
Run `bun run` to list all tasks.

## Explore

[Roadmap](ROADMAP.md) · [Design decisions](docs/README.md) · [Examples](examples/) ·
[React + Vite](examples/vite-react/README.md) ·
[DOM bindings](web/README.md) · [Browser tests](browser/README.md) ·
[Compiler in WebAssembly](wasm/README.md)
