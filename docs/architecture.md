# Architecture

How rust-js is put together: what each part does, what it may not do, and
where a change goes. The [design decisions](README.md) say why each part
works the way it does; this page says where it is. The boundaries below
are executable: [architecture.test.ts](../test/architecture.test.ts)
checks each one.

## The pipeline

rust-js reuses rustc for everything up to type checking and borrow
checking, as ReScript reuses OCaml's, and writes JavaScript from rustc's
THIR:

```text
 .rs ─► rustc: parse, resolve, type check, borrow check ─► THIR (copied out)
                                                              │
 ┌─ front end: src/lower.rs, src/lower/ ──────────────────────▼──────────────┐
 │  analysis  ─► what the whole crate is: dictionaries, drops, laziness ...  │
 │  pipeline  ─► each function, lowered with its own FnCx                    │
 │  THIR expression ─► expr() or stmt() ─► our JS AST (src/js.rs)            │
 └──────────────────────────────────┬────────────────────────────────────────┘
                                    ▼  owned output: no rustc types from here
 reachability, link, names, prepare (readability), output, publish
                                    ▼
 printing: to_oxc.rs ─► oxc ─► format.rs ─► .js + .js.map
```

Four layers, each allowed to depend only on what's left of it:

| Layer | Modules | May not |
|---|---|---|
| Driver | `main.rs`, `cargo.rs` | |
| Front end | `lower.rs`, `lower/`, `jsx_syntax.rs` | publish files; link |
| Owned output | `js.rs`, `program.rs`, `link.rs`, `reachability.rs`, `names.rs`, `prepare.rs`, `output.rs`, `publish.rs`, `manifest.rs`, `library.rs`, `runtime.rs`, `settings.rs`, `hooks.rs` | use rustc |
| Printing | `to_oxc.rs`, `format.rs` | be bypassed: only they use oxc |

## Inside the front end

The front end turns one function's THIR into JS. `FnCx` holds what that
takes. Its state is grouped by concern: what a generic item is given
(`Given`), what writing to a `Formatter` knows (`display::Writing`), what
iterator chains are beyond their types (`iterators::Chains`), what walks of
types found (`TypeWalks`), and what's dropped where (`drops::DropState`).

Its modules come in four kinds.

**Questions, which emit nothing.** They read THIR and types, and answer.
Each is checked to stay that way.

| Module | Answers |
|---|---|
| `recognition.rs`, `recognition/` | Which std function or method a call is, a `Std`: `classify` asks what's known by identity, then a trait's method, then a type's own |
| `body_queries.rs` | What a body has: `for` loops, `.await`, places, stepped iterators |
| `effects.rs` | What evaluating an expression, or calling a closure, can do that can be seen |
| `drops/types.rs` | What dropping a type runs |
| `drops/facts.rs` | What a body owns and moves, found before it's lowered |
| `analysis.rs` | What the whole crate is, before any function is lowered |

**Lowering of Rust's constructs.**

| Module | Lowers |
|---|---|
| `lower.rs` | Dispatch of expressions and statements, destinations, evaluation order |
| `bodies.rs` | Functions, closures and nested bodies: setup, captures, entry and exit |
| `patterns.rs` | Bindings, destructuring, `match`, `if let` and let-chains |
| `loops.rs` | Loops and labels |
| `places.rs` | Reading, borrowing and writing places |
| `drops.rs` | Destructors: scopes, flags, temporaries, and the JS that drops |
| `representation.rs` | How each Rust value is represented, copied and validated |
| `traits.rs`, `std_impls.rs`, `ordering.rs` | Traits: dictionaries, evidence, `dyn`; `Clone`, `Default`, `PartialEq`, `Ord` |
| `display.rs`, `format_args.rs`, `format_spec.rs` | `Display` and `Debug`, `format_args!`, placeholders' options |
| `serde.rs`, `serde/` | serde's derives and serde_json |
| `jsx.rs`, `jsx_api.rs`, `bindings.rs` | JSX, and bindings to JavaScript |
| `library.rs`, `sources.rs`, `pipeline.rs` | Libraries' contracts, source files, and the crate as a whole |

**Std's functions.** A call is lowered by `calls.rs::call`, a short
dispatcher:

```text
 call(f, args)
   ├─ the crate's own function, a binding, a closure, a trait's method
   │     └─► special_call
   └─ one of std's: classify(f) = Some(known)
         └─► std_call(known): what every std call must keep (drops, Option boxing)
               └─► the domain that knows it, asked in turn:
                     vecs.rs  options.rs  cells.rs  numbers.rs
                     iterators.rs  text.rs  format_args.rs
                     maps.rs  ranges.rs  combinators.rs  serde/value.rs ..
```

Each domain's function lowers its `Std` variants and says `None` to the
rest. `std_call`'s own match names every variant a domain lowers, so a new
`Std` variant is a compile error until something lowers it.

**Iterators.** Whether a value is a JS iterator or an array is one
question, `iterators::is_lazy_value`: its type says so (an endless source,
an iterator of the crate's own), or a chain's consumer made it so, when a
stage does what can be seen ([ADR 0139](decisions/0139-lazy-chains.md)).
Only `iterators.rs` reads either answer.

**Runtime helpers.** The JS that generated code imports from
`@rust-js/runtime`: each is a file, `src/runtime/<name>.js`, which
`runtime.rs` names and orders, and the package is made from them
([ADR 0103](decisions/0103-runtime-package.md)).

## Where a change goes

A std function or method rust-js doesn't know yet:

1. **Recognize it**: a `Std` variant, and where `classify` finds it, in
   `recognition.rs` or `recognition/methods.rs`.
2. **Lower it** in its domain's function: `vecs.rs` for a `Vec`'s, `text.rs`
   for a string's, and so on. If it needs a runtime helper, add
   `src/runtime/<name>.js` and name it in `runtime.rs`.
3. **Prove it**: a corpus case in `test/corpus/`, compared with native Rust;
   mutations in its module's list in `scripts/mutations/`, each a bug its
   tests must catch;
   and a design decision in `docs/decisions/` if it's a new choice.

A new kind of value, construct or analysis goes with its kind above: a
question that emits nothing in a module of its own, checked as the others
are, and lowering in the module of the construct. Check the generated JS
reads as a person would write it, and run the checks in
[AGENTS.md](../AGENTS.md), in its Linux VM.
