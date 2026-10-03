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
takes. Its state is grouped by concern, each group read and written only
by the module that owns it, which others ask: what a generic item is given
(`Given`), what writing to a `Formatter` knows (`display::Writing`), what
iterator chains are beyond their types (`iterators::Chains`), which locals
are stepped through (`iterators::Stepping`), what the `&mut`s to values JS
can't change in place are (`mut_refs::MutRefs`, in `Locals`), what walks of
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
| `analysis.rs`, `analysis/` | What the whole crate is, before any function is lowered: what it refuses, the names its items have in JS, the types it changes in place, its drops' and `Debug`'s needs |
| `shortcuts.rs` | Nothing of its own: `self.is_map(ty)` for `self.recognition().is_map(ty)`, each question answered in `recognition.rs` |

**Lowering of Rust's constructs.**

| Module | Lowers |
|---|---|
| `lower.rs` | Dispatch of expressions and statements, destinations, evaluation order |
| `bodies.rs` | Functions, closures and nested bodies: setup, captures, entry and exit |
| `patterns.rs` | Bindings, destructuring, `match`, `if let` and let-chains |
| `loops.rs` | Loops and labels |
| `places.rs` | Reading, borrowing and writing places, and `mem::swap` and `replace` of them |
| `mut_refs.rs` | A `&mut` to a value JS can't change in place, given to a call or given back by one: a box, the place, or a std call's items |
| `drops.rs` | Destructors: scopes, flags, temporaries, and the JS that drops |
| `representation.rs` | How each Rust value is represented: its shape, an `Option`'s payload, a cell, a number |
| `copies.rs` | When a value is copied: where it's read, if something changes one of its kind in place |
| `support.rs` | What rust-js can represent, and the error where a value it can't is made or bound |
| `aggregates.rs` | Structs, variants and tuple structs made, by fields or a struct update, and constructors as values |
| `results.rs` | `?`, and the `From` that converts its error |
| `items.rs` | References to items: what a function or a binding is in JS, whether it's the crate's, which impl a call runs |
| `traits.rs`, `std_impls.rs`, `ordering.rs` | Traits: dictionaries, evidence, `dyn`, calls of an impl's method; `Clone`, `Default`, `PartialEq`, `Ord` |
| `display.rs`, `format_args.rs`, `format_spec.rs` | `Display` and `Debug`, `format_args!`, placeholders' options |
| `serde.rs`, `serde/` | serde's derives and serde_json |
| `jsx.rs`, `jsx_api.rs`, `bindings.rs` | JSX, and bindings to JavaScript |
| `library.rs`, `sources.rs`, `pipeline.rs` | Libraries' contracts, source files, and the crate as a whole |

**Who owns what.** A concept that more than one module needs has one
module that owns it: its state, the questions about it, and the JS it's
written as, which the others ask for. [architecture.test.ts](../test/architecture.test.ts)
holds the ones below to it, and `bun run architecture` shows where each
stands: what's read outside its owner, and how much each module knows of
the rest.

| Concept | Owner | What the others ask |
|---|---|---|
| What dropping runs, and where | `drops.rs`, `drops/` | `drop_value`, `moved`, `temporary`, `enter_body_drops` |
| Whether a value is a JS iterator or an array | `iterators.rs` | `is_lazy_value`, `mark_lazy_chain`, `dyn_iterator` |
| Locals stepped through, `$iter`s, and a generic iterator as a JS iterator (ADRs 0061, 0071) | `iterators.rs` | `steps_through`, `stepped_value`, `is_stepping`, `js_iterator`, `stepped_items` |
| `&mut`s to values JS can't change in place | `mut_refs.rs` | `is_boxed`, `is_alias`, `slot`, `is_item_call` |
| Writing to a `Formatter` | `display.rs` | `written`, `formatter_answer`, `with_dyn_debug` |
| What a function or a binding is in JS | `items.rs` | `fn_ref`, `js_ref`, `resolve_instance` |
| Dictionaries and evidence | `traits.rs` | `dictionary`, `evidence_for`, `impl_call` |
| What a generic function was given: dictionaries, a copied default's arguments, type facts (ADRs 0049, 0145) | `traits.rs` | `in_impl_terms`, `given_evidence`, `given_type_fact`, `resolve_self_instance`, `enter_default` |
| The box of a `Some` that looks like `None` (ADR 0051) | `options.rs` | `some`, `some_value`, `some_literal`, `is_some_box` |
| When a value is copied | `copies.rs` | `copy_if_needed`, `contains_mutated` |
| A cell, a lock, and the place its guard names (ADRs 0025, 0144) | `cells.rs`; `places.rs` for the place | `ref_place`, `guarded_cell` |
| Which std function a call is | `recognition.rs` | `classify`, and its answers through `shortcuts.rs` |
| Std's names: which std item a type, trait or function is | `recognition.rs` | `StdItem`, `is_std_type`, `is_std_def`, `std_item`, `trait_method` |

**Calls.** A call is lowered by `calls.rs::call`, a short dispatcher;
what a function is in JS is `items.rs`'s, and a `&mut` given to one
`mut_refs.rs`'s:

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
