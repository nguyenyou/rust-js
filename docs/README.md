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

[architecture.md](architecture.md) says what's inside `src/lower/`, the
boundaries between the parts, and where a change goes.

| File | Job |
|---|---|
| `src/main.rs` | CLI, rustc callbacks, analysis and the diagnostic gate |
| `src/jsx_syntax.rs`, `src/jsx_syntax/` | Load configured modules and expand JSX tokens into typed Rust before resolution; shared by native and WASM |
| `src/lower/analysis.rs` | Collect named items, imports, trait and mutation facts |
| `src/lower/pipeline.rs` | Orchestrate emission, reachability and owned symbolic module assembly |
| `src/reachability.rs` | Traverse dependencies from caller-selected roots without frontend knowledge |
| `src/link.rs`, `src/names.rs` | Resolve module imports, collision-free aliases and runtime dependencies after lowering |
| `src/lower.rs`, `src/lower/` | Crate facts, function lowering, bindings, representations and JSX semantics |
| `src/lower/serde.rs`, `src/lower/serde/`, `serde/` | `#[serde(..)]` attributes, and JSON written and read as serde_json does; the serde crates, built with the pinned toolchain |
| `src/runtime/from_json.js` | serde_json's reader, ported to JS |
| `src/runtime.rs`, `src/runtime/` | Runtime helpers, each a file of its own, the `@rust-js/runtime` package's (`runtime/`) |
| `src/prepare.rs` | JSX readability preparation after lowering |
| `src/program.rs`, `src/lower/sources.rs` | Owned linked modules and multi-file source origins |
| `src/output.rs`, `src/manifest.rs` | Validated artifact plans and the versioned build result |
| `src/publish.rs` | Native publication, ownership and rollback |
| `tooling/` | Native build adapter, manifest consumers and WASI host publication |
| `wasm/web/compiler-client.js`, `wasm/web/compiler-worker.js` | Browser compiler lifecycle and recovery |
| `src/js.rs` | Our small JS AST; every node carries a Rust span |
| `src/to_oxc.rs` | Converts to oxc's AST, prints, builds the source map |
| `src/format.rs` | Formats the printed JS as oxfmt does, with a crate's options, and moves the source map to match, or to a transform's text of the same program |
| `src/settings.rs`, `src/hooks.rs` | A crate's settings in its `Cargo.toml`, and the transforms and checks it runs on the JS, what they say moved to the Rust (0117) |
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
- [0086 An `i64` or a `u64` is a BigInt, wrapped as release Rust wraps it](decisions/0086-64-bit-integers.md)
- [0087 `println!` is `console.log`; `print!` writes as it is where JS can](decisions/0087-printing.md)
- [0090 rustc checks programs for `wasm32-unknown-unknown`, whose `usize` is rust-js's](decisions/0090-wasm32-front-end.md)
- [0065 The JS is formatted as oxfmt formats it, and the source map follows](decisions/0065-format-with-oxfmt.md)
- [0066 Text with values in it is a template literal](decisions/0066-template-literals.md)
- [0067 Range patterns, `@`, `let ... else`, and a `&mut` into a map](decisions/0067-patterns.md)
- [0069 Preserve effects before simplifying; lower once and link afterwards](decisions/0069-lowering-effects-and-linking.md)
- [0070 A std function taken as a value is an arrow](decisions/0070-function-values.md)
- [0071 An iterator stepped through is a `$iter`, which knows where it is](decisions/0071-stepping-iterators.md)
- [0074 A `&mut` to a string or a number is a box the caller copies back](decisions/0074-mut-boxes.md)
- [0076 A struct's `..base` after its fields, and std's own steps for a few more methods](decisions/0076-std-odds.md)
- [0068 `VecDeque` and `BinaryHeap` are arrays; a heap moves its items as Rust's does](decisions/0068-queues.md)
- [0015 Loops: put `while` back, label only when needed](decisions/0015-loops.md)

**Web programs**

- [0021 JS interop: `extern` blocks name what JS has](decisions/0021-js-interop.md)
- [0022 Closures are arrow functions](decisions/0022-closures.md)
- [0023 Strings, references and shared state](decisions/0023-strings-references-shared-state.md)
- [0024 The `webapi` crate: DOM bindings generated from WebIDL](decisions/0024-web-crate.md)
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
- [0072 JSX syntax in the native and browser compilers](decisions/0072-jsx-syntax.md)
- [0075 JSX is the public syntax for React elements](decisions/0075-jsx-only-elements.md)

**Talking to a server**

- [0077 `#[derive(Serialize)]` is a function that writes serde_json's text](decisions/0077-serde-json.md)
- [0078 `serde_json::from_str` is serde_json's reader, ported; `#[derive(Deserialize)]` is a table it reads by](decisions/0078-serde-json-reading.md)
- [0079 A tagged or untagged enum is read as serde reads one: through the value, read first and kept](decisions/0079-serde-tagged-enums.md)
- [0080 A generic type's codec takes its type parameters' codecs; `Result`, `from`, `try_from` and `into` are serde's](decisions/0080-serde-generics-and-conversions.md)
- [0081 A `T: Serialize` bound's evidence is `T`'s writer, and `T: DeserializeOwned`'s its reader](decisions/0081-serde-bounds.md)
- [0082 `#[serde(flatten)]` is serde's flat map: a field's entries among its struct's](decisions/0082-serde-flatten.md)
- [0083 serde_json's `Value` is an enum like any other; its `Number` is `{ kind, value }` and its `Map` a `Map`](decisions/0083-serde-json-value.md)

**Scope and process**

- [0016 Only top-level functions, for now](decisions/0016-crate-shape.md) *(modules: superseded by 0019)*
- [0017 Test against native Rust, not against expectations](decisions/0017-differential-testing.md)
- [0088 The corpus: Rust programs that say what they expect, run natively and as JS](decisions/0088-corpus.md)
- [0089 rustc's own `run-pass` tests run as JS, with a list of what fails that only shrinks](decisions/0089-rustc-tests.md)
- [0091 What rust-js writes depends only on what it's given](decisions/0091-build-history.md)
- [0092 Generated programs, each from a seed, and reduced when they fail](decisions/0092-generated-programs.md)
- [0093 Known bugs put back into the compiler, which the tests must catch](decisions/0093-mutations.md)
- [0094 A distribution is qualified by the suite, run through what it installs](decisions/0094-qualification.md)
- [0095 The JS rust-js makes runs on Node; Bun runs the tests](decisions/0095-node-runtime.md)
- [0096 A `static` is its module's value, and a `static mut` its `{ value }`](decisions/0096-statics.md)
- [0097 A user `Deref` or `IndexMut` is its method, and an auto trait's impl is nothing](decisions/0097-deref-and-marker-impls.md)
- [0098 A destructor runs where rustc runs it, in a `finally`](decisions/0098-destructors.md)
- [0099 A `&mut` held in a variable names its place; one kept elsewhere is a handle](decisions/0099-mut-references.md)
- [0100 Each crate is compiled once, to JS of its own, with a manifest of what crosses](decisions/0100-separate-crates.md)
- [0101 Cargo builds a workspace with rust-js as its workspace wrapper](decisions/0101-cargo-workspace-wrapper.md)
- [0102 Two binding crates, `js` and `webapi`, named as ReScript's](decisions/0102-js-and-webapi.md)
- [0103 The runtime is a package, `@rust-js/runtime`, as ReScript's is](decisions/0103-runtime-package.md)
- [0104 The tests run side by side, each native program built once](decisions/0104-parallel-tests.md)
- [0105 `bun create @rust-js` makes a Vite and React app in Rust](decisions/0105-create.md)
- [0106 A trait's type parameters, associated types, generic methods and constants](decisions/0106-generic-traits.md)
- [0107 A const parameter is a value its caller gives](decisions/0107-const-generics.md)
- [0108 Operators and `Into` in generic code are dictionaries](decisions/0108-generic-operators-and-into.md)
- [0109 Pin a stable release: rust-js checks code as the Rust most people run does](decisions/0109-stable-release.md)
- [0110 rust-js's syntax is stable Rust's](decisions/0110-stable-syntax.md)
- [0111 A JS type is a struct of a `JsObject`](decisions/0111-js-types-as-structs.md)
- [0112 rust-js compiles the binding crates](decisions/0112-rust-js-compiles-the-bindings.md)
- [0113 A plain rustc compiles a program too](decisions/0113-plain-rustc.md)
- [0114 An app's Rust is a Cargo package, for editors](decisions/0114-app-cargo-toml.md)
- [0115 The binding crates are packaged for crates.io](decisions/0115-binding-crates-on-crates-io.md)
- [0116 How bindings are versioned](decisions/0116-binding-versions.md)
- [0117 A crate's settings for the JS rust-js writes: its formatter's options, and hooks](decisions/0117-output-hooks.md)
- [0118 Bindings on npm only, as ReScript's are](decisions/0118-bindings-on-npm-only.md)
- [0119 Bindings the community maintains, in one repository](decisions/0119-community-bindings.md)
- [0120 The first npm release: 0.0.1, for macOS on Apple silicon](decisions/0120-first-npm-release.md)
- [0121 A map keyed by a struct is a `Map` keyed by its value](decisions/0121-value-keys.md)
- [0122 An `f32` is a JS number, each result rounded with `Math.fround`](decisions/0122-f32.md)
- [0123 A slice's pattern tests its length and its items](decisions/0123-slice-patterns.md)
- [0124 A `|` pattern binds each name where its alternative has it](decisions/0124-or-pattern-bindings.md)
- [0125 A constructor, or a closure as a `fn`, is a JS function](decisions/0125-function-values.md)
- [0126 A byte string is its bytes, and `as_bytes()` a string's UTF-8 bytes](decisions/0126-byte-strings.md)
- [0127 An inline `const` is its value, and an `if let` guard binds for its arm](decisions/0127-const-blocks-and-let-guards.md)
- [0128 std's iterator sources: `once` is an array, and `repeat` a JS iterator](decisions/0128-iterator-sources.md)
- [0129 A range is a value: `{ start, end }`, iterated as its items](decisions/0129-range-values.md)
- [0130 An `if let` is a value anywhere, and trait and std functions are values](decisions/0130-if-let-values-and-fn-items.md)
- [0131 A temporary taken apart owns what its pattern leaves](decisions/0131-temporaries-taken-apart.md)
- [0132 A `Box` from its value, `Default` of a `&str`, `type_name`, and standard streams](decisions/0132-std-odds.md)
- [0133 Impls whose names would be the same are named by their arguments](decisions/0133-impl-names-apart.md)
- [0134 A `let` of a pattern without a value declares each of its variables](decisions/0134-let-patterns-without-values.md)
- [0135 A trait's const parameter is its impl's, and a trait method's is given](decisions/0135-const-generic-traits.md)
- [0136 Common std methods: `take`, `cmp::max`, ASCII case, and `Debug`'s builders](decisions/0136-common-std-methods.md)
- [0137 `{:#?}` is pretty Debug, and a writer is told whether](decisions/0137-pretty-debug.md)
- [0138 A string's length, slices and offsets count its UTF-8 bytes](decisions/0138-string-byte-counts.md)
- [0139 A chain whose stages do what can be seen runs in Rust's order](decisions/0139-lazy-chains.md)
- [0140 An iterator trait object is a JS iterator](decisions/0140-iterator-trait-objects.md)
- [0141 A `dyn Display` or `dyn Error` is a value and its dictionary](decisions/0141-std-trait-objects.md)
- [0142 A channel on one thread is a queue its ends share](decisions/0142-channels.md)
- [0143 A `Formatter`'s options are an object its writers are given](decisions/0143-formatter-options.md)
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

**Architecture boundaries**

- [0084 Owned compiler phases and explicit host boundaries](decisions/0084-owned-phases-and-host-boundaries.md)
- [0085 Experimental scalar library linkage](decisions/0085-scalar-library-linkage.md)
