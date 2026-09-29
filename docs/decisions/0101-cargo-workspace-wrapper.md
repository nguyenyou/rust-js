# 0101. Cargo builds a workspace with rust-js as its workspace wrapper

Status: Accepted. Extends [0100](0100-separate-crates.md), and replaces
[0085](0085-scalar-library-linkage.md)'s planning of a Cargo graph for
building one.

## Context

ADR 0100 compiles each crate on its own, dependencies first, to JS, its
metadata and a manifest. Something must run those compilations in order,
with each crate's flags: its edition, features, `cfg`s and the `--extern`
of each dependency. Cargo already works all of that out, for crates of the
workspace and of crates.io alike, and runs build scripts and procedural
macros on the host.

`tooling/cargo.js` (ADR 0085) reads Cargo's graph and would run rust-js
itself. It refuses registry crates and procedural macros, so `models`,
deriving serde's traits, can't be built with it.

## Decision

rust-js is Cargo's `RUSTC_WORKSPACE_WRAPPER`, as Clippy is:

```
RUSTC_WORKSPACE_WRAPPER=rust-js cargo check --target wasm32-unknown-unknown -p frontend
```

Cargo runs `rust-js <rustc> <flags>` for each member of the workspace, and
`rustc <flags>` for everything else. Running as a wrapper is known by the
first argument, a path whose name is `rustc`:

```
cargo ──► rust-js rustc --crate-name validation --crate-type lib ...
            │
            ├─ a probe (`-`, `-vV`, `--print`), not a library, or not for
            │  wasm32-unknown-unknown ──► rustc, as asked
            │
            └─ a library of the workspace for rust-js's target ──► rust-js,
               into Cargo's <out-dir>, with <hash> Cargo's for this build:
                 libvalidation-<hash>.rmeta                (Cargo's --extern)
                 libvalidation-<hash>.rust-js              (where the manifests are)
                 validation-<hash>.d                       (Cargo's dep-info)
                 rust-js/validation-<hash>/lib.js, lib.manifest.json
```

- **A crate's JS is beside its metadata, one for each build of it Cargo
  keeps.** Cargo keeps a build for each feature set, profile and target,
  and runs nothing for one it has as done. Written one place for all of
  them, the JS would be the last build's, whichever Cargo then found
  done. A consumer imports a library's JS where it is (ADR 0100).
- **The wrapper is the switch:** Cargo rebuilds what it checked with rustc
  once rust-js is its wrapper, and the other way round.
- **Each is a library, with `--library`,** the package Cargo was asked for
  too. Built as an app, it would be done, and still an app, when a later
  build uses it. Cargo can't be told which it was: it drops what it set for
  rustc itself, `CARGO_PRIMARY_PACKAGE`, from what it tracks.
- **What rustc writes is what Cargo expects:** the metadata where Cargo
  looks for it, and its dep-info. Cargo's `--emit` is replaced by those two.
- **`cargo check`, not `cargo build`:** a build asks rustc for what it links,
  and is refused. And only a build is pipelined: Cargo starts a crate's
  dependents once rustc says its metadata is written, before rust-js would
  publish it with the JS. A check's dependents start when rustc ends.
- **A crate's dependencies are found by the marker beside their metadata:**
  a `.rust-js` file with its manifest's path, then those of the libraries
  it was compiled with. Every manifest in the markers of a crate's
  `--extern`s is a `--dependency`, so a library it uses through another is
  the build that one was compiled with (ADR 0100). A crate without a marker
  is one rustc built, serde say. A manifest in a marker that's gone is
  refused, with `cargo clean` as the remedy: compiled as if it were
  rustc's, its consumer would be wrong.
- **Cargo's record of the sources lists rust-js too:** Cargo rebuilds a
  crate when a file it lists is newer than the build.
- **What Cargo is told is published with the JS,** the marker and the
  dep-info, staged as the metadata is: from one plan, or none of it
  (ADR 0100). Written after, a failure to write them would leave Cargo
  told the build failed, beside the new JS.

A crate's JS is where Cargo's hash for its build says, so
`checkCargo` (`tooling/cargo.js`) runs `cargo check --message-format=json`
and says where. Cargo reports each crate's metadata, built or found done,
and the marker beside it says where its manifest is, which says where its
JS is. Cargo checks its own outputs, not rust-js's, so `checkCargo` checks
each file a crate's manifest lists, of a build Cargo has as done too,
against the fingerprint it was published with. One gone or changed is
refused, with `cargo clean -p <package> --target wasm32-unknown-unknown`
as the remedy, not given to an app that imports it.

## Why

- **Cargo's flags are Cargo's:** features, `cfg`s, editions, renamed
  dependencies and every `--extern` come as Cargo resolves them, without
  rust-js reading `Cargo.toml` or `cargo metadata`.
- **Cargo's cache is the JS's:** what Cargo keeps a build of, its metadata,
  has JS of its own, so a build Cargo has as done has the JS it was.
- **Registry crates, build scripts and procedural macros work as they do
  for Clippy:** rustc builds them. serde's derive runs on the host, and
  serde and serde_json are the metadata rust-js reads (ADR 0077).
- **A workspace wrapper, not `RUSTC_WRAPPER`:** only the workspace's crates
  are rust-js's to compile.

## Alternatives

- **An output directory of the user's (`RUST_JS_OUT`),** a crate's JS in
  `<dir>/<crate>/`: the first version of this ADR. A crate's builds shared
  it, and Cargo, finding one done, ran nothing to write its JS back. Nor
  could Cargo see the directory set once a crate was checked without it.
  Found in review.
- **Planning the graph ourselves (`tooling/cargo.js`, ADR 0085),** then
  running rust-js for each library. It would repeat what Cargo decides
  about features and flags, and it can't build a registry crate or a
  procedural macro without becoming Cargo.
- **`cargo build` with a linker for JS:** Cargo would ask rustc for
  machine code and link it. rust-js writes JS from THIR, not from codegen.
- **Tracking the JS as one of Cargo's outputs:** Cargo sets the dep-info's
  time to when the build began, and a file written during the build, the
  JS, would be newer than that on every run.

## Consequences

- **The proof** (`test/crates.test.ts`): ADR 0100's `validation`, `models`
  and `frontend` as a workspace, with serde from crates.io, and a native
  `check` binary calling `frontend::main`. Built with `cargo check`, the
  JS prints what `cargo run -p check` does, and after an edit to
  `validation`, what the edited program does. `models` built alone first
  is a library when `frontend` uses it, and `shell`, which names only
  `frontend`, is told of the rest. An unchanged build rewrites no JS, a
  library whose manifest is gone is refused, and so is `cargo build`.
- **Found in review, each a test:** a feature set, another, and the first
  again, each its own JS; a crate checked by rustc, then with rust-js as
  the wrapper, compiled by it; a marker that can't be written, which
  leaves the previous build's JS; and a library's JS deleted from a build
  Cargo has as done, refused until `cargo clean -p` builds it again. Each rule above has a mutation the tests
  catch.
- **Found by the proof:** an import took the name of the crate's own
  function, and it was the crate's that was renamed, `main$1`, so JS
  calling `shell.main` found none. A crate's items are now named first,
  and imports around them (`test/crates/pairs/same_name`).
- **The JS is Cargo's, in its target directory:** `cargo clean` removes it
  with the metadata, and a bundler is given the app's path by `checkCargo`,
  which changes with Cargo's hash.
- **A registry crate used at run time is rustc's,** so a call into it is
  refused, as a call into any crate without a manifest is, unless rust-js
  knows it: serde's derives and `serde_json` (ADR 0077). Compiling registry
  crates with rust-js too is a decision of its own.
- **Every workspace library exports what its consumers can reach,** the
  app too (ADR 0100): its JS is a library's.
