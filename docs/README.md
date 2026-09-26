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
                                 to_oxc.rs: JS AST ──► oxc AST           (0018)
                                       │
                                       ▼
                                 oxc_codegen ──► one .js + .js.map per module  (0018, 0019)
```

## Code map

| File | Job |
|---|---|
| `src/main.rs` | Hooks into rustc's driver, runs analysis, writes the files |
| `src/lower.rs` | Turns THIR into the JS AST: the actual compiler |
| `src/js.rs` | Our small JS AST; every node carries a Rust span |
| `src/to_oxc.rs` | The only oxc code: converts, prints, builds the source map |
| `test/native.rs`, `test/fib.test.ts` | Differential test: native Rust vs. generated JS, plus source map checks |
| `test/sourcemap.ts` | A tiny source map decoder for the tests |

## Decisions

Each record says what we decided, why, what we rejected, and what it costs.

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
- [0020 Structs are objects, tuples are arrays](decisions/0020-structs-and-tuples.md)
- [0014 `match` becomes an `if`/`else if` chain](decisions/0014-match-lowering.md)
- [0015 Loops: put `while` back, label only when needed](decisions/0015-loops.md)

**Web programs**

- [0021 JS interop: `extern` blocks name what JS has](decisions/0021-js-interop.md)
- [0022 Closures are arrow functions](decisions/0022-closures.md)
- [0023 Strings, references and shared state](decisions/0023-strings-references-shared-state.md)
- [0024 The `web` crate: DOM bindings generated from WebIDL](decisions/0024-web-crate.md)
- [0025 `Vec`, `for` loops, `RefCell` and `&mut` to objects](decisions/0025-vec-loops-refcell-mut.md)
- [0028 Imports from JS modules: `#[link_name = "module#path"]`](decisions/0028-js-module-imports.md)
- [0029 `async`/`.await` are JS's `async`/`await`; a future is a promise](decisions/0029-async-await.md)
- [0030 `Option`: `Some(x)` is `x`, `None` is `undefined`](decisions/0030-option.md)
- [0031 A `const` is the value rustc computed, under its own name](decisions/0031-consts.md)

**Scope and process**

- [0016 Only top-level functions, for now](decisions/0016-crate-shape.md) *(modules: superseded by 0019)*
- [0017 Test against native Rust, not against expectations](decisions/0017-differential-testing.md)
- [0026 Tests are Rust's `#[test]`, run by `bun test` in happy-dom](decisions/0026-testing.md)
- [0027 Real-browser tests: Playwright Test and Vitest's browser mode, on Bun](decisions/0027-real-browser-tests.md)
- [0032 The playground is written in Rust, compiled by rust-js, a part at a time](decisions/0032-dogfooding-the-playground.md)

## Research

Explorations that aren't decisions yet:

- [An in-browser rust-js playground](research/in-browser-playground.md): run rustc's front end + rust-js as WebAssembly

## Adding a decision

Copy the shape of an existing record: **Context → Decision → Why →
Alternatives → Consequences**. Number it next in sequence. If a new decision
replaces an old one, don't delete the old one. Set its status to
`Superseded by NNNN`, so the history of *why* survives.
