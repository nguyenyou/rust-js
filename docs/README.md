# rust-js design docs

rust-js compiles Rust to readable JavaScript. The whole design follows from one
sentence, borrowed from ReScript:

> Keep the language's own front end. Replace only the back end. Where the
> language and JS disagree, pick the JS behavior that keeps the output small
> and readable, and write that choice down.

This folder is where we write those choices down.

## The pipeline in one picture

```
 fib.rs
   │
   ▼
 rustc front end  (parse, expand macros, resolve names, type check)
   │
   ├──► THIR of every function ──copy──┐        (0004)
   │                                   │
   ▼                                   │
 rustc analysis (borrow check, lints)  │
   │                                   │
   ├── any error? ──► stop, write nothing  (0006)
   │                                   │
   ▼                                   ▼
 Compilation::Stop               lower.rs: THIR ──► JS AST, with spans  (0008–0015)
 (no rustc codegen)                    │
                                       ▼
                                 prepare.rs: readability preparation
                                       |
                                       v
                                 to_oxc.rs: JS AST ──► oxc AST           (0018)
                                       │
                                       ▼
                                 oxc_codegen ──► one .js + .js.map per module  (0018, 0019)
```

## Code map

| File | Job |
|---|---|
| `src/main.rs` | CLI, rustc callbacks, analysis and the diagnostic gate |
| `src/lower/link.rs` | Resolve actual module dependencies and collision-free aliases after lowering |
| `src/lower.rs`, `src/lower/` | Crate facts, function lowering, bindings, representations and JSX semantics |
| `src/runtime.rs` | Runtime helpers emitted on demand |
| `src/prepare.rs` | JSX readability preparation after lowering |
| `src/output.rs` | Filename validation, manifests and artifact publication |
| `src/js.rs` | Our small JS AST; every node carries a Rust span |
| `src/to_oxc.rs` | Converts to oxc's AST, prints, builds the source map |
| `src/format.rs` | Formats the printed JS as oxfmt does, and moves the source map to match |
| `test/native.rs`, `test/compiler.test.ts` | Differential test: native Rust vs. generated JS |
| `test/emission.test.ts`, `test/diagnostics.test.ts` | Source maps, manifests, output ownership and compiler rejections |
| `test/react.test.ts`, `test/browser.test.ts`, `test/vite.test.ts` | React behavior, browser runners, and real Vite/Fast Refresh |
| `test/sourcemap.ts` | A tiny source map decoder for the tests |

## Decisions

Each record says what we decided, why, what we rejected, and what it costs.

- [0042 Compiler boundaries and build-tool manifest](decisions/0042-compiler-boundaries-and-build-contract.md)

**Foundation**

- [0001 Reuse rustc's front end](decisions/0001-reuse-rustc-front-end.md)
- [0002 Generate JS from THIR, not MIR](decisions/0002-generate-from-thir.md)
- [0003 Pin one nightly and link rustc's internals](decisions/0003-pin-nightly-toolchain.md)
- [0004 Copy THIR before analysis, stop before codegen](decisions/0004-driver-hook.md)
- [0005 One file in, one ES module out](decisions/0005-input-and-output.md)
- [0006 Only programs rustc accepts become JS](decisions/0006-errors-and-unsupported-features.md)

**Code generation**

- [0007 A JS AST with a precedence-aware printer](decisions/0007-js-ast-and-printer.md) *(printer half superseded by 0018)*
- [0018 Print with oxc, through one adapter file, and emit source maps](decisions/0018-print-with-oxc.md)
- [0008 Two lowering modes: expressions and statements](decisions/0008-expression-and-statement-modes.md)
- [0009 Temporaries keep Rust's evaluation order](decisions/0009-temporaries-and-evaluation-order.md)
- [0010 Unique names per function, flat blocks](decisions/0010-naming-and-scopes.md)
- [0019 One JS file per Rust module](decisions/0019-one-js-file-per-module.md)

**Semantics**

- [0011 Integers are JS numbers, wrapped like release Rust](decisions/0011-numbers.md)
- [0012 Panics throw, via runtime helpers emitted on demand](decisions/0012-panics-and-runtime-helpers.md)
- [0013 Fieldless enum variants are strings](decisions/0013-fieldless-enums.md)
- [0033 Enums with fields are ReScript's tagged objects](decisions/0033-enums-with-fields.md)
- [0035 JS that throws is a `Result`; `?` returns early](decisions/0035-results-and-throwing-js.md)
- [0020 Structs are objects, tuples are arrays](decisions/0020-structs-and-tuples.md)
- [0049 Trait dictionaries, generics, and read-only trait objects](decisions/0049-traits-and-generics.md)
- [0047 Methods are an object of functions named after their type](decisions/0047-methods.md)
- [0014 `match` becomes an `if`/`else if` chain](decisions/0014-match-lowering.md)
- [0048 Let chains: each part runs only once the ones before it held](decisions/0048-let-chains.md)
- [0051 `Option<T>` in generic code: boxed only when it looks like `None`](decisions/0051-generic-options.md)
- [0052 The crate's own `Default`, `From` and `Clone`, and the trait ABI kept](decisions/0052-std-trait-impls.md)
- [0053 `==`: JS's `===` or `$eq`, until a hand-written `eq` is in it](decisions/0053-partial-eq.md)
- [0054 `Display`: a `fmt` returns the string it writes](decisions/0054-display.md)
- [0055 The crate's own `Iterator` is a JS iterator](decisions/0055-iterator.md)
- [0056 Indexing: `$index(v, i)` to read, `v[$at(v, i)] = x` to write](decisions/0056-indexing.md)
- [0057 `PartialOrd` and `Ord`: an `Ordering`, and the parts in turn](decisions/0057-ordering.md)
- [0058 Format options, where Rust applies them](decisions/0058-format-options.md)
- [0059 `HashMap` is a JS `Map`, `HashSet` a `Set`, keyed by value](decisions/0059-hashmap.md)
- [0060 `{:?}` by the type, and a derived `Debug` is a function](decisions/0060-debug.md)
- [0061 `impl Iterator` is the type it hides; a generic iterator is any JS iterable](decisions/0061-generic-iterators.md)
- [0062 Combinators and adapters: the closure's body in place](decisions/0062-combinators.md)
- [0063 `char`'s questions are Unicode regular expressions; `parse` is a `Result` of Rust's message](decisions/0063-text.md)
- [0064 Numbers' methods are `Math`'s, where JS agrees; operators call their impl](decisions/0064-numbers.md)
- [0065 The JS is formatted as oxfmt formats it, and the source map follows](decisions/0065-format-with-oxfmt.md)
- [0066 Text with values in it is a template literal](decisions/0066-template-literals.md)
- [0067 Range patterns, `@`, `let ... else`, and a `&mut` into a map](decisions/0067-patterns.md)
- [0069 Preserve effects before simplifying; lower once and link afterwards](decisions/0069-lowering-effects-and-linking.md)
- [0070 A std function taken as a value is an arrow](decisions/0070-function-values.md)
- [0068 `VecDeque` and `BinaryHeap` are arrays; a heap moves its items as Rust's does](decisions/0068-queues.md)
- [0015 Loops: put `while` back, label only when needed](decisions/0015-loops.md)

**Web programs**

- [0021 JS interop: `extern` blocks name what JS has](decisions/0021-js-interop.md)
- [0022 Closures are arrow functions](decisions/0022-closures.md)
- [0023 Strings, references and shared state](decisions/0023-strings-references-shared-state.md)
- [0024 The `web` crate: DOM bindings generated from WebIDL](decisions/0024-web-crate.md)
- [0025 `Vec`, `for` loops, `RefCell` and `&mut` to objects](decisions/0025-vec-loops-refcell-mut.md)
- [0034 String methods are JS's; a `char` is a one-character string; `format!` is `+`](decisions/0034-strings-and-chars.md)
- [0036 An iterator is a JS array; `Ordering` is -1, 0 or 1](decisions/0036-iterators-and-sorting.md)
- [0037 `thread_local!` is a variable of its module](decisions/0037-thread-locals.md)
- [0028 Imports from JS modules: `#[link_name = "module#path"]`](decisions/0028-js-module-imports.md)
- [0029 `async`/`.await` are JS's `async`/`await`; a future is a promise](decisions/0029-async-await.md)
- [0030 `Option`: `Some(x)` is `x`, `None` is `undefined`](decisions/0030-option.md)
- [0031 A `const` is the value rustc computed, under its own name](decisions/0031-consts.md)
- [0038 Variables have JS's names and shapes: `const [count, setCount] = ..`](decisions/0038-js-names-and-destructuring.md)
- [0039 Generic bindings: `#[rust_js::link_name]` on an ordinary function](decisions/0039-generic-bindings.md)
- [0046 `#![rust_js::camel_case]`: a crate's own names, the JS way](decisions/0046-camel-case-crates.md)

**React**

- [0040 JSX: bindings whose `link_name` is a tag, printed as JSX in a `.jsx` file](decisions/0040-jsx.md)
- [0041 React: the `react` crate, and Vite with Fast Refresh](decisions/0041-react.md)
- [0043 React's whole API, gated by the release that added it](decisions/0043-react-versions.md)

**Scope and process**

- [0016 Only top-level functions, for now](decisions/0016-crate-shape.md) *(modules: superseded by 0019)*
- [0017 Test against native Rust, not against expectations](decisions/0017-differential-testing.md)
- [0050 Snapshots of the generated JS, reviewed as diffs](decisions/0050-snapshots.md)
- [0026 Tests are Rust's `#[test]`, run by `bun test` in happy-dom](decisions/0026-testing.md)
- [0027 Real-browser tests: Playwright Test and Vitest's browser mode, on Bun](decisions/0027-real-browser-tests.md)
- [0032 The playground is written in Rust, compiled by rust-js, a part at a time](decisions/0032-dogfooding-the-playground.md)
- [0044 The playground on React, one component per file](decisions/0044-playground-on-react.md)
- [0045 The playground is a Vite app, with React Compiler and Tailwind](decisions/0045-playground-on-vite.md)

## Research

Explorations that aren't decisions yet:

- [An in-browser rust-js playground](research/in-browser-playground.md): run rustc's front end + rust-js as WebAssembly

## Adding a decision

Copy the shape of an existing record: **Context → Decision → Why →
Alternatives → Consequences**. Number it next in sequence. If a new decision
replaces an old one, don't delete the old one. Set its status to
`Superseded by NNNN`, so the history of *why* survives.
