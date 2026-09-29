# 0100. Each crate is compiled once, to JS of its own, with a manifest of what crosses

Status: Accepted. Extends [0085](0085-scalar-library-linkage.md), [0049](0049-traits-and-generics.md) and [0098](0098-destructors.md).

## Context

A full-stack app shares crates between its server and its client:

```
frontend (rust-js) ─┐
                    ├─► shared-models ─► validation
server   (native) ──┘
```

Today a crate can only be shared as source, a module included by `#[path]`
in both, or as a library of free functions of `bool`, `i32` and `u32`
(ADR 0085). Models are structs and enums with derives and methods, so
neither is enough.

Two designs are known, and compilers to JS have taken each:

- **Separate compilation:** each library is JS of its own, which a
  consumer imports as it is: ReScript, Gleam, PureScript.
- **Separate checking, whole-program linking:** each library is an
  intermediate form, and the app's build links them all and writes the
  JS once: Scala.js (`.sjsir`), Kotlin/JS (klibs).

Linking would need an intermediate form of our own. rust-js writes JS from
THIR, which rustc never gives for another crate; its metadata has MIR only
for generic and inline functions, all of them only with
`-Zalways-encode-mir` on every crate and std, and then as control flow,
not the structured code readable JS comes from.

Separate compilation fits Rust better than it looks. A generic function is
compiled once and given dictionaries (ADR 0049), so a consumer never needs
a library's bodies, as rustc does to copy them for each type. And Cargo has
the source of every dependency, so every crate can be compiled by the same
compiler, as Gleam and PureScript compile theirs: there's no ABI to keep
between compiler versions.

What doesn't fit is what rust-js decides from the whole crate. Each is
exact within one crate, and each goes wrong across two:

- **A `Copy` type no code changes in place is read without a copy.** A
  library whose `origin()` returns its constant `ORIGIN` as it is gives it
  to a consumer that writes `p.x = 5`, and `ORIGIN` changes. So does a
  clone: a consumer that never changes a `User` itself clones one as the
  same object, and the library's `birthday(&mut self)` changes both.
- **A generic function takes a `dropT` only if a caller in the crate gives
  it a value with a destructor** (ADR 0098). A consumer is a caller the
  library never saw.
- **A derived impl no function in the crate reaches is left out,** and so
  is a derived serde codec no function uses. A consumer may use them.

## Decision

**Each crate is compiled once, by rust-js, to JS of its own, the metadata
its consumers' rustc reads, and a manifest of what they need to know.**
Crates are compiled in Cargo's order, dependencies first:

```
validation ─► shared-models ─► frontend

each library:  rust-js ─► <out>/<crate>/lib.js, one file for each module (ADR 0019)
                       ─► <out>/<crate>/lib<crate>.rmeta     its metadata
                       ─► <out>/<crate>/lib.manifest.json   what crosses
each consumer: rustc checks it against those `.rmeta`s, and rust-js reads the manifests
frontend.js:   import { User } from "../shared_models/lib.js";
```

**A library's metadata is rust-js's own.** With `--emit=metadata=<path>`
among rustc's flags, rustc goes on, after rust-js has lowered the crate, to
write its metadata, from that same compilation. It has to be: the key a
consumer finds an item by, rustc's `DefPathHash`, depends on the flags the
crate was built with, `-C metadata` among them, and the crate is as rust-js
configured it, `cfg(rust_js)` and all, which is what a consumer must be
checked against. rustc writes it where rust-js stages it, beside the JS,
and it's published at the path asked for as an artifact of the same plan
as the JS (ADR 0091): checked not to be a source or another output,
fingerprinted in the manifest, and published or rolled back with the rest.
A library's JS and its metadata are one build's, or neither is. rustc's
other outputs, `--emit=mir` say, however spelled, are refused: they'd be
written past these checks, and aren't anything rust-js makes.

**A consumer works out from a library's types what the same compiler
would; anything else it's told.** A struct's object, an enum's `TAG`, an
`Option` as its value or `undefined`, and the dictionaries a generic
function takes follow from the types and signatures rustc's metadata
gives, so they're worked out again, the same way. What rust-js decides
from a library's bodies, or from its whole crate, is in the manifest.

**The manifest (version 2) is:**

- **Whose it is:** the crate's name, its crate hash (`tcx.crate_hash`, the
  SVH rustc checks between crates), the rust-js compiler that wrote it,
  and the manifest's version.
- **Its items, by `DefPathHash`:** each function, method and impl method a
  crate outside can reach, with the module file it's in and its JS name:
  `validate` of `User` (ADR 0047), `userDebug_fmt` (ADR 0049), and the
  serde functions a derive made. A name is written as the library chose
  it, and never worked out again.
- **What each function takes that its signature doesn't say:** which type
  parameters it's given a `dropT` for (ADR 0098).
- **The trait impls whose methods those are,** which a consumer calls, not
  what a derive would be: an `==` written by hand is the impl's, not its
  fields'.
- **The files it was made from, and what it wrote,** fingerprinted as
  ADR 0085's are.

A consumer imports what it uses by that name, as it imports a JS module's
(ADR 0028), and calls it as it calls its own: with boxes for `&mut`
numbers (ADR 0074), dictionaries, and drops. An item the manifest doesn't
list is an error that names it.

**A crate a library's consumers can reach assumes the worst of what
crosses:**

- **A type that can cross a crate boundary might be changed in place,**
  so it's copied when read, as a type this crate changes is: a library's
  type, in any crate, and a library's own type its consumers can reach.
  A type private to one crate keeps the copies it doesn't need, and a
  fieldless enum, a JS string, never needs one.
- **So might any `Vec`, tuple or array, in a library or a crate using
  one:** a library's own method may push into a `Vec` inside a value its
  consumer cloned, which the consumer never takes `&mut` of itself, and a
  tuple is a JS array whichever crate's it is, `ORIGIN: (i32, i32)` too.
- **A public generic function takes a `dropT` for each type parameter a
  caller could give a value with a destructor,** one that isn't `Copy`.
  A caller whose `T` has none passes nothing, which `dropT?.(x)` already
  handles.
- **A derived impl or codec a consumer can reach is kept and exported.** A
  codec rust-js can't write, of a type it can't read or write, is then an
  error when the library is compiled, not when a consumer uses it.

**A library's type, trait and destructor are rust-js's, as the crate's own
are.** Where rust-js asks whether something is the crate's own, or std's,
a library's is neither, and is taken as the crate's own would be:

- **A trait of a library's is passed as dictionaries** (ADR 0049), and the
  dictionary of an impl of it is the library's, imported. A library and
  its consumers must agree on which traits are, for every function that
  takes one, so each manifest lists the libraries its crate was compiled
  against, and a consumer without the manifest of one is refused.
- **A library's type's `Drop` is imported and run,** as the crate's own is
  (ADR 0098). Another crate's destructor, of one that isn't std's or in
  rustc's sysroot, as `hashbrown`, which std's maps are made of, is an
  error: what it runs is out of sight.
- **A library's `Copy` enum is copied, and its recursive type cloned by a
  function that calls itself,** as the crate's own are (ADRs 0033, 0074).
- **A library's generic function as a value is given its dictionaries,**
  in an arrow, as the crate's own is.

What a crate can reach is rustc's own answer: its effective visibilities,
which follow `pub use` too. The app's own crate, which no one imports,
keeps every one of today's decisions.

**A consumer checks its dependencies before it compiles:** each manifest's
version and compiler must be its own, each file it lists unchanged, and
each crate hash the one rustc loaded from that crate's metadata, so a
manifest left beside the metadata of another build can't pass for it. A
mismatch is an error, and nothing is written. **A crate is rebuilt
whenever a crate it depends on is,** directly or not: a consumer's
manifest lists the library's files, so one made from an older build is
refused until it's rebuilt too.

**No runtime helper is shared.** Each crate carries the helpers it uses,
as each module does. Two copies of a class could tell each other apart,
but the only ones compared across a call, serde's `$JsonError` and its
decoders, never cross: a library's codec reads through the decoder its
caller made, `json.int("u32", ..)`, so an error it finds is the caller's,
caught where it's thrown.

**A `&mut` parameter crosses as it's passed within a crate:** the object,
or a box (ADR 0074). ADR 0099's handles have the same `value`.

**Not yet:**

- **A driver:** the proof compiles each crate in order itself, as Cargo's
  graph gives them (`tooling/cargo.js`, ADR 0085); running rust-js as
  Cargo's `RUSTC_WORKSPACE_WRAPPER`, as Clippy runs, would give each crate
  its Cargo flags.
- **Implementing another crate's trait that has default methods:** a
  default's body is copied into each impl (ADR 0049), and a consumer can't
  read the library's. The library could export each default as a generic
  function over `Self`; that's a decision of its own.
- A library's `static` used from another crate: its one value is the
  library's, which a consumer would import. A `const` is its value, as
  within a crate (ADR 0031), and a copy of it the consumer's own.
- Two versions of one crate in one build, still an error (ADR 0085).
- A stable API for hand-written JS importing a library's modules: the
  shape of its JS belongs to the compiler that wrote it.
- Build scripts, and crates from a registry.

## Why

- **It's what the three languages that do it best do,** each for the
  reason that applies here: Gleam and PureScript compile every package
  from source with one compiler, and rebuild what imports a rebuilt one;
  ReScript writes the representation into the interface, and falls back
  to the safe form where the interface can't say.
- **Every fact is either a type's, a signature's, or the manifest's.**
  rustc does the same for what a downstream crate needs from a body: it
  computes it upstream, for each item, and writes it in the metadata.
- **It's exact.** Where a library can't know what a consumer does, it
  assumes the worst; what it does know, the consumer is told.
- **The app's JS stays as it is.** Only what crosses pays for crossing,
  with copies a person would write anyway to be safe.

## Alternatives

- **Linking, as Scala.js and Kotlin/JS do:** a crate's IR would keep only
  what's true of the crate, and the app's build would decide the rest, for
  JS as good as one crate's. It needs an IR of our own, with its format,
  versions and checker, a link in every build, and caches to keep that
  fast. It could come later, for a release build, over what this writes.
- **A library saying which of its types nothing changes,** as rustc ships
  its facts about each function: but whether a type changes is decided by
  its consumers, and a library can only speak for itself.
- **Metadata from a separate `rustc` run, or `cargo check`:** it's another
  build, whose keys and crate hash aren't the JS's.
- **Rebuilding only when an interface changes:** faster, but a fact outside
  the types, a `dropT` say, changes without one. ReScript's constants,
  inlined from beside the interface, are left stale this way.
- **A stable ABI across compiler versions:** it would let a crate ship its
  JS, at the cost of freezing every representation. Cargo has the sources,
  so nothing needs it.

## Consequences

- **The proof** (`test/crates.test.ts`, of `test/crates/`): `validation`, of
  free functions of strings and numbers returning `Result`; `models`, of a
  struct and enums deriving `Debug`, `Clone`, `Copy`, `PartialEq` and
  serde's traits, methods, a constant, an `==` written by hand, a `Vec` its
  method changes, and a generic function; and `frontend`, which uses both.
  Compiled each on its own, it prints what the same crates do natively,
  `ORIGIN`, a clone and its `birthday`, a `Bag`'s clone, a destructor of
  `frontend`'s run by `models`, a JSON round trip and two errors among it.
  An edit to `validation` is refused until what uses it is rebuilt, and
  then it's what the app does; a manifest beside another build's metadata
  is refused. Each rule above has a mutation that the proof catches.
- **Two crates at a time** (`test/crates/pairs/`), each found in review:
  a library's destructor, a tuple constant, a trait's dictionary for a
  generic function and for a generic function as a value, a `Copy` enum, a
  recursive clone, a generic impl's dictionary, and one given a drop. And
  a consumer missing a library's manifest, metadata that can't be
  published, JS that can't, and metadata asked for over a source, each
  refused with nothing written.
- A library's JS copies more, and its generic functions take more, than
  the same code in one crate would, and each crate carries the runtime
  helpers it uses: `models`' serde codecs bring the JSON reader's.
- ADR 0085's scalar contract was manifest version 1; version 2 replaces it,
  and its tests take the new one.
- **Of rustc's tests,** two that passed are now refused, each for what its
  generic impl's drop needs and rust-js doesn't do yet: dropping an
  `Option<T>`, and a `dyn` of a value with a destructor. They had passed
  because their `T` happens to drop nothing.
