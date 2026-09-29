# 0084. Owned compiler phases and explicit host boundaries

Status: Accepted. Refines 0009, 0042, 0045 and 0069.

## Decision

Keep one compiler crate. Separate responsibilities through owned values and
module APIs rather than introducing a plugin framework or a second Rust IR.

- `program.rs` owns completed modules, imports, test descriptions, and source
  text. Printing, preparation, artifact planning and publication do not depend
  on rustc. `lower/sources.rs` translates rustc spans into an owned source arena.
  The printer restores individual source identities, including a trait default
  copied from another file. Macro spans still use their original callsite.
- Cross-module expressions carry `js::Symbol`, never sentinel strings that
  resemble JavaScript names. Linking reserves local names, assigns imports,
  and consumes symbols before printing. An unresolved symbol is an internal
  compiler invariant violation, never valid generated JavaScript. The resolver
  lives in `src/link.rs` and accepts only owned modules, symbols, import candidates
  and reserved names. Lowering translates rustc module identities into symbols
  before this boundary. `src/names.rs` owns the shared identifier and collision
  policy. Neither linking nor name allocation depends on rustc or lowering.
  `lower/pipeline.rs` returns owned `Unlinked` modules with explicit import
  requests, reserved names and requested runtime helpers. The driver invokes
  `link::link` only after the lowering diagnostic gate passes. Linking consumes
  this value, resolves imports and runtime dependency closure, and produces the
  `Linked` wrapper accepted by output planning. Its private constructor stays
  in the linker; the wrapper owns the existing `Lowered` value without copying. `lower/analysis.rs` returns an
  `AnalyzedCrate` containing named items, imports, trait facts, body references,
  tests and mutation facts. It does not lower functions or invoke the linker.
  The result owns its collections and borrows captured THIR; it is not a second
  Rust IR or a frontend-independent artifact. The pipeline retains diagnostic
  accumulation and the existing demand-driven codec/reachability ordering.
  Dependency traversal lives in `src/reachability.rs` with opaque identity inputs
  and no rustc or emission dependencies. The pipeline chooses roots using the
  existing derived-implementation policy; the traversal handles cycles and
  transitive dependencies iteratively. This is not general dead-code elimination
  or cross-crate linkage.
- Operand lowering produces prerequisite statements and a value. Sequencing
  uses those actual statements to capture earlier operands before executing
  later prerequisites. `is_simple` is no longer the operand-order oracle:
  a call that looks simple can still introduce statements. Immutable and
  borrowed places retain the existing Rust-checked exemptions.
  For JSX, capture the element's inputs rather than the element itself: keep
  nested JSX in the returned tree. Property reads, spread copies, mutable
  component selections and child computations retain their original order;
  conditional inputs remain inside their selected branches. Static inputs and
  immutable local bindings need no temporary.
  The same rule applies to ternary candidates and `matches!` guards. Lower each
  branch once into an `Evaluation`. Keep a ternary (or `&&` guard) only when
  its conditional evaluations have no prerequisite statements; otherwise emit
  an `if` with branch-local setup and result assignment. A primitive `&mut`
  argument needs a box and copy-back even when its Rust call looks simple.
- Keep lowering dispatch and sequencing in `lower.rs`; put function and nested
  body lifecycle in `lower/bodies.rs`, patterns and bindings in `patterns.rs`,
  loops in `loops.rs`, place operations in `places.rs`, and arithmetic/casts
  alongside numeric methods in `numbers.rs`. These remain parts of the same
  lowerer, with shared `FnCx` state; splitting files does not make them
  independent compiler phases.
  `lower/body_queries.rs` is a narrower boundary: only immutable THIR and rustc
  queries, with no emission context. Each captured `Body` owns context-independent
  variable-use counts, mutably borrowed roots and stepped iterators, collected
  once. Nested-body entry switches the current facts and exit restores them.
  Type/representation caches still depend on the active typing environment and
  trait arguments and are not added to these facts.
  `PreparedPlace` gives ordinary assignment targets, map entries and borrowed
  map-value slots a common read/write contract after preparation. Preparation
  sequences the RHS before a primitive assignment's target and captures target
  operands when they cannot safely be read twice. Map checks and slot write-back
  remain explicit. Overloaded assignment calls retain receiver-before-argument
  order; assignments that drop the old value retain their cleanup path.
- `lower/recognition.rs` classifies standard-library calls from a definition ID
  and generic arguments. Its context contains only rustc type queries, the typing
  environment, and immutable local trait-implementation identities. It cannot
  access function locals, dependency recording, temporary allocation, statement
  output, or runtime-helper selection. Shared type/trait predicates live there;
  lowering delegates to them instead of duplicating recognition rules. Formatting
  and iterator emission remain in `stdlib.rs`. Serde Value calls also use this
  read-only boundary: conversion, indexing, comparison (including operand order
  and negation), default construction and inherent-method ownership are recognized
  before operands are lowered. Value/Number inherent methods produce typed
  operations too, including an explicit unsupported operation for useful
  diagnostics. Emission consumes those operations without method-name dispatch.
  Value conversion and scalar comparison categories also come from recognition;
  emission constructs the selected representation and recursively lowers container
  conversions. The Option type predicate is shared with ordinary lowering.
  Formatting calls, formatter signatures and standard Serde skip predicates also
  use recognition. Local formatter-variable validation and calls to user-defined
  predicates stay in emission because they require function state.
  The final cleanup also moves parse/JSON error and Reverse identity checks,
  iterator classification, Serde trait/derive and conversion-target recognition,
  supported trait classification, and Into/TryInto resolution into this boundary.
  Comparisons and fallible JS bindings return typed operations; lowering retains
  operand sequencing, local implementation availability and runtime selection.
  Numeric, text and combinator method tables live under `recognition/methods.rs`.
  Read-only HIR queries may accumulate local visitor results, but do not mutate
  the recognition context or access emission state. The mutable-map lookup
  special case and Ordering constants use the same identity boundary.
- A body lowered inside another, a closure's, an `async fn`'s coroutine or
  a trait default copied into an impl, starts and ends through `enter_body`
  and `leave_body`: `Nested` says, in one place, what each kind shares with
  the enclosing body and what it gets of its own, and each is checked for
  what it drops (ADR 0098). Found in review: each path swapped its own set
  of fields, and a closure's and a copied default's stepped iterators (ADR
  0071) weren't their own, so `it.next()` in one was rejected.
  What a body knows of its variables, each one's JS meaning and what was
  found of some (boxes, aliases, slots, iterators), is one `Locals`: a
  closure and a coroutine share their enclosing body's, a copied default
  has its own, and what's added to it goes with it. What a nested body
  took is one `EnclosingKind` for each kind, which `leave_body` gives
  back as `enter_body` took it. Found in review: `aliases` (ADR 0099) was
  added, and nothing said whether a copied default had its own.
- Struct-update scratch values are invocation-local. A discarded call receives
  that destination explicitly; its argument calls still produce their values.
- `runtime.rs` owns helper dependency closure and stable emission order. Feature
  handlers request helpers rather than reproducing dependency lists. Heap
  operations and Serde Value lowering request only their direct helper; the
  catalog supplies sifting and JSON support dependencies. Isolated heap-module
  tests check both behavior and omission of unrelated sifting helpers. Substantial
  JSON and fixed-format implementations live in JavaScript files. Helpers remain
  inline per module; sharing state or moving helpers into runtime modules needs
  a separate measured change.
- `output.rs` constructs a complete artifact plan. `publish.rs` owns filesystem
  writes and rollback. The native publication guarantee remains ordinary I/O
  recovery, not crash-atomic replacement of multiple files.
- `manifest.rs` defines the producer schema; `tooling/manifest.js` validates it
  for hosts. Version 1 gains an optional compiler identity (release, exact Rust
  pin, ABI 1), so existing version-1 consumers remain compatible. ABI 1 names
  the current output contract; it does **not** establish a cross-crate ABI.
  Source dependencies list files actually loaded, not source-path records
  imported from rustc metadata. Virtual paths are remapped field by field.
  `--version-json` exposes the same compiler identity before compilation. Hosts
  validate its ABI and require packaged resources to match the compiler release
  and Rust pin before preparing bindings. Development checkout resources keep
  their existing workflow; release identity does not replace artifact checksums.
- `tooling/build.js` prepares native compilation. Vite owns scheduling, watching,
  overlays and refresh. The `rust-js-build` package exposes build, manifest, and
  publication entry points; Vite and the playground use declared dependencies
  rather than imports outside their package directories. Local tarball tests
  exercise the plugin outside the checkout with an explicit compiler path.
  `scripts/package-resources.ts` packages binding inputs and the Rust pin into
  a separate versioned source-resource tarball. Packaging and metadata cache
  invalidation share the inventory in `tooling/resources.js`. Isolated package
  tests compile JSX and Serde using the unpacked resources. Native compiler
  binaries, installation, and release qualification remain distribution work.
  Hosts prefer explicit resource paths, then resolve the resource package from
  the application's dependencies, with checkout fallback for development. The
  package test installs local tarballs offline through Bun, repeats with a frozen
  lockfile, and compiles through plugin hooks without a resource-path override.
  A local `rust-js-native` package pairs the native executable with a JavaScript launcher
  that locates pinned toolchain libraries. Hosts hash/watch the executable as
  well as the launcher, including when configured through the `.bin` symlink.
  This has development-host installation coverage, not clean-machine portability
  or a qualified binary release.
  Distributed tooling uses standard Node.js APIs and supports Node.js (ADR 0095).
  Hosts invoke the packaged launcher and React configuration helper with their
  current runtime. Bun remains a repository development tool, not a distribution
  requirement. Package tests execute each runtime while blocking the other on PATH.
  Compiler discovery follows the same application boundary: explicit path,
  installed `rust-js-native`, then checkout fallback. Native hosts and Vite use
  the same resolver. Installed-package tests require no compiler or resource
  path overrides; explicit `.bin` paths remain supported.
  `scripts/package-distribution.ts` assembles the four version-matched packages
  in a staging directory and exposes the completed bundle with one rename.
  It rejects existing destinations and writes a distribution manifest and
  SHA-256 checksums. This is local artifact assembly, not registry publication,
  publisher authentication, or release qualification.
  Hosts can select compiler/resources/cache locations,
  built-in React and Serde preparation, explicit extern metadata and rustc flags. React
  metadata caches are keyed by compiler bytes, binding inputs, resource root,
  React version and options. A completion marker is written only after success.
  This is a binding cache, not a Cargo build cache.
  Serde preparation invokes pinned Cargo with the bundled locked manifest and
  reads artifact paths from JSON messages. It runs Cargo's freshness check on
  reuse and includes the manifest, lockfile and preparation source in the cache
  key. `bindings: ["react", "serde"]` is shared by native hosts and Vite; this
  supplies only the already-supported Serde subset, not arbitrary Cargo crates.
- The WASI build host publishes only manifest-listed artifacts and removes only
  stale files whose ownership fingerprints still match. It stages changes and
  publishes its manifest last, with rollback on ordinary I/O failure. It never
  scans for arbitrary JavaScript files to delete.
- **Both hosts remove a stale file by the same rules:** a generated `.js`,
  `.jsx` or `.map`, inside the output's directory, that this build neither
  writes nor reads (its input and sources, through symlinks), with the bytes
  it was written with. `test/publication.test.ts` runs each case against the
  native compiler and the WASI host's publisher. Found in review: the WASI
  host checked only the fingerprint, and removed a file the next build read
  as a source.
- The playground runs compilation in a disposable worker. Cancel, timeout,
  worker errors and message failures terminate that worker and yield a failed
  result; a later compile starts fresh. The editor retains the downloaded
  module and dependency bytes. Each worker receives its own dependency data.
- Preview frames use `sandbox="allow-scripts"` without same-origin access.
  Messages must come from the current frame, match the run ID, and pass shape
  validation before becoming UI state. A malformed message cannot silence the
  missing-report timeout. This isolates editor DOM/storage; it does not impose
  a network or resource quota on preview programs.

## Evidence

`test/architecture.test.ts` guards rustc and oxc dependency boundaries and
compiler dependency versions. `test/semantics.test.ts` compares nested updates,
discarded calls, and operand prerequisites with native Rust. Generated snapshots
record the additional temporaries needed to retain evaluation order.
The [conditional-regions corpus](../../test/corpus/conditional_regions.rs)
compares skipped branches, boxed mutable arguments, guarded patterns, nesting,
condition effects and repeated loop execution with native Rust. Its snapshot
keeps the generated branch structure reviewable. The `conditional-prerequisites`
mutation restores unconditional hoisting, which this case must catch.
Architecture tests also prohibit emission state in `body_queries.rs`.
The [prepared-places corpus](../../test/corpus/prepared_places.rs) checks
statement-valued writes through borrowed map entries and primitive versus
overloaded compound-assignment order against native Rust.

`test/emission.test.ts` covers copied-body source origins and native output
ownership. `test/manifest.test.ts` rejects malformed/incompatible manifests and
compiles an independent temporary application through the build adapter.
`test/shared-code.test.ts` builds a separate native executable and generated JS
from one shared model/validation module, checks JSON exchanges and malformed
requests, and verifies rebuilds after shared-source edits. This proves source
sharing through modules, not a Cargo dependency graph or an HTTP/browser pilot.
`test/host-boundaries.test.ts` covers WASI host ownership, staging failure and
untrusted preview payloads. `test/playground.test.ts` checks a real worker,
cancellation followed by recovery, the preview origin boundary, and freshly
built native/WASI output parity.

The Check workflow is manual-only to avoid automatic GitHub Actions costs during
development. Its separate WASM job
builds from candidate sources and requires playground parity rather than silently
skipping missing WASM. Repository branch-protection settings remain external to
this checkout; the workflow alone does not enforce merging policy.

## Remaining target-architecture work

The frozen local Cargo planner now discovers dependency order, aliases and
features. [ADR 0085](0085-scalar-library-linkage.md) adds a separate-compilation
proof for scalar functions, with input/artifact checks and native comparison.
General Cargo build orchestration, reusable compilation caches, exported trait
and representation metadata and a general cross-crate ABI remain open.
Local packages can run outside the source checkout; clean-machine
distribution and release qualification remain open. Neither are
additional serialization frameworks or Protocol Buffers. These are feature and
release work under roadmap M3/M4/M8, not consequences of moving modules.

Library recognition still has concrete Serde/std assumptions. Extending those
families requires explicit recognition, representation and semantic tests; there
is no claim that adding a serializer is a configuration-only operation.
The recognition cleanup for currently supported families is complete. Architecture
tests reject library crate-name checks outside recognition (except the scalar
linkage adapter's canonical crate identity) and method tables in emission modules.
`test/recognition.test.ts` compares same-named user types, real standard types,
and ordering operations against native Rust. Serde-specific representation and
attribute rules remain in its adapter deliberately: these define encoding and
emission rather than identify a library operation. Rust syntax/desugaring checks,
diagnostic names and associated-item lookup are not library-name dispatch.
The enforced ownership, publication, rustc and printer boundaries remain intact.

Application-scale phase/memory budgets, runtime-sharing policy, and an independent
client/server adoption test also remain open. The existing module-graph benchmark
is useful evidence for that workload, not a general scalability guarantee.

Current module-graph measurement (`bun run bench:lowering`, debug compiler,
three warm samples on the development machine): 10 modules 33 ms; 100 modules
136 ms; 500 modules 1,326 ms. This includes rustc, formatting and publication.
These are observations, not enforced budgets or comparisons across machines.

Verification in this checkout: `RUST_JS_REQUIRE_WASM=1 bun test` passed all 302
tests (7,628 expectations) after `bun run wasm`, including recognition boundary
guards and native comparisons for same-named user types.
`bun run fmt:check`,
`cargo clippy --locked -- -D warnings`, `cargo test --locked`, documentation
link checks and `git diff --check` also passed. Generated JavaScript snapshots
remain unchanged. Hosted CI was not run.
