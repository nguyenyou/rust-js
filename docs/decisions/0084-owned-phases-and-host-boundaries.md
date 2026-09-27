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
  compiler invariant violation, never valid generated JavaScript.
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
- Struct-update scratch values are invocation-local. A discarded call receives
  that destination explicitly; its argument calls still produce their values.
- `runtime.rs` owns helper dependency closure and stable emission order. Feature
  handlers request helpers rather than reproducing dependency lists. Substantial
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

General Cargo graph resolution, reusable compiled JS crates, exported trait and
representation metadata, a validated cross-crate ABI, and distribution without
a compiler-resource checkout are not implemented by this refactor. Neither are
additional serialization frameworks or Protocol Buffers. These are feature and
release work under roadmap M3/M4/M8, not consequences of moving modules.

Library recognition still has concrete Serde/std assumptions. Extending those
families requires explicit recognition, representation and semantic tests; there
is no claim that adding a serializer is a configuration-only operation.

Application-scale phase/memory budgets, runtime-sharing policy, and an independent
client/server adoption test also remain open. The existing module-graph benchmark
is useful evidence for that workload, not a general scalability guarantee.

Current module-graph measurement (`bun run bench:lowering`, debug compiler,
three warm samples on the development machine): 10 modules 33 ms; 100 modules
136 ms; 500 modules 1,326 ms. This includes rustc, formatting and publication.
These are observations, not enforced budgets or comparisons across machines.

Verification in this checkout: `RUST_JS_REQUIRE_WASM=1 bun test` passed all 273
tests (7,346 expectations) after `bun run wasm`. `bun run fmt:check`,
`cargo clippy --locked -- -D warnings`, `cargo test --locked`, documentation
link checks and `git diff --check` also passed. Generated JavaScript snapshot
changes were reviewed. Hosted CI has not been run for these uncommitted changes.
