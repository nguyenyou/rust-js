# 0042. Compiler phases and a manifest for build tools

Status: Accepted. Extends 0006, 0018, 0019, 0040 and 0041.

## Context

JSX adds output extensions and presentation choices; Vite adds repeated builds.
The original lowering file also owned crate discovery, bindings, representations
and runtime source. Printing during filesystem writes allowed a late output
collision to leave partial files. A malformed side-effect import could report an
error during lowering and still reach emission.

## Decision

Keep one compiler crate and the shared native/WASI entry point. Give each phase
an explicit responsibility:

| Owner | Responsibility |
|---|---|
| `main.rs` | CLI, rustc callbacks, analysis and the final diagnostic gate |
| `lower/analysis.rs` | Discover definitions and module relationships; collect immutable crate facts; orchestrate lowering |
| `lower.rs` | Function state, control flow, patterns, expression evaluation order and places |
| `lower/representation.rs` | Supported types, numeric and aggregate representations, constants and copying |
| `lower/bindings.rs` | Decode and validate binding attributes |
| `lower/calls.rs`, `lower/stdlib.rs` | Dispatch calls and implement supported library operations |
| `lower/jsx.rs` | JSX semantics, including evaluation order when builder order differs from JSX syntax |
| `runtime.rs` | Helpers selected by lowering and emitted on demand |
| `js.rs` | Shared JavaScript/JSX tree, independent of rustc and oxc |
| `prepare.rs` | Readability transformations on completed JS trees; preserve evaluation regions and source spans |
| `to_oxc.rs` | The only oxc integration; conversion, layout and source maps |
| `output.rs` | Plan and validate filenames, generate artifacts, publish files and track ownership |

`CrateFacts` is borrowed read-only by function contexts. Keep evaluation-order
machinery shared: feature modules must not invent their own argument-order rules.
No lowering module predicts printer indentation. Semantic temporaries remain in
lowering; optional JSX readability temporaries belong to preparation. Preparation
must not move expressions outside conditional/repeated regions, ahead of earlier
effects, or across potentially observable property reads.

### Output contract

The existing command line remains valid. Build tools can additionally request:

```sh
rust-js src/App.rs -o src/App.js --manifest target/app-manifest.json -- --extern react=target/libreact.rmeta -L target
```

The version-1 JSON manifest records:

- `input`, `output`: the input and requested output paths.
- `sources`: source files loaded by rustc, including modules emitting no JS.
- `modules`: Rust module path segments, final `file`, `map`, `source`, and generated module `imports`.
- `artifacts`: generated file paths and stable content fingerprints.

Native paths are absolute. The requested `output` can end in `.js` while the
actual root module's `file` ends in `.jsx`. Consumers must use final filenames,
not infer them from the request. Fingerprints are ownership checks, not security
hashes. The manifest does not list itself as a generated artifact.

Validate all final paths, including the test harness and manifest, before
printing or writing. Any Rust/lowering diagnostic prevents output. Generate all
artifacts in memory, stage native files beside their destinations, then publish;
unchanged files retain their timestamps. Keep originals for rollback on an I/O
error and publish the manifest last. This is **not crash-atomic across files**.

On successful recompilation, remove obsolete files only when the previous
manifest belongs to the same input/output pair, they are inside the output
folder, and their contents still match the recorded fingerprint. Preserve
user-edited and unrelated files. Without a manifest, no stale-file cleanup occurs.

WASI uses fresh virtual filesystems whose hosts may not support rename or
realpath. It performs the same validation and generation, writes directly, and
its caller exposes results only on success. Native staging is not required there.

### Vite contract

The plugin requests manifests and watches their source dependencies, including
additions and deletions. Missing/new dependencies cause affected or failed roots
to be retried. Compiler and binding-source changes invalidate metadata and rebuild
all roots. Content-based metadata inputs replace the old one-time Boolean.

One asynchronous queue serializes builds and coalesces save events. Compilation
errors preserve existing artifacts and appear in Vite's overlay. Successful
recovery clears the overlay even when generated code is unchanged. Stable output
IDs use ordinary React Fast Refresh; a change to the output module set or suffix
invalidates IDs and reloads the page. The plugin aliases `.js`/`.jsx` requests to
the actual generated module so an entry import survives an extension transition.

The plugin still uses the compiler and binding sources from this checkout.
Packaging/distribution and the browser playground's worker/module loader are
separate work.

## Why

These boundaries keep language semantics, readable output and host integration
from changing one another accidentally. They also let tests exercise compiler
rejections, output publication, React behavior and Vite recovery independently.
The refactor preserves the existing public bindings and readable-output fixtures.

## Alternatives

- Split into many Cargo crates: unnecessary packaging and API work for the same compiler.
- Keep formatting decisions inside lowering: couples correctness-sensitive expression movement to printer layout.
- Infer dependencies and output extensions in Vite: duplicates information the compiler already owns.
- Delete every old `.js` file on rebuild: risks deleting user files and unrelated crate output.

## Consequences

Tests live in separate compiler, diagnostic, emission, React, browser and Vite
suites with shared build helpers. PR CI runs the native checks, browser suites
and a production example build. The WASI package shares the source and declares
the same new JSON dependency; its lockfile remains independently pinned.

Whole-crate THIR collection and mutation analysis remain unchanged. This work
improves responsibility boundaries and rebuild behavior; it does not establish
large-input performance or independent compilation of Rust crates.
