# 0003. Pin one nightly and link rustc's internals

Status: Accepted

## Context

To read THIR we link against rustc's own crates (`rustc_driver`,
`rustc_middle`, ...) using `#![feature(rustc_private)]`. That API is internal:
it changes between nightlies without notice, and a rename can break us any day.

## Decision

- `rust-toolchain.toml` pins **`nightly-2026-03-25`** (rustc commit
  `362211dc2`), with the `rustc-dev`, `llvm-tools` and `rust-src` components.
- `build.rs` bakes the toolchain's `lib/` directory into the binary's rpath.
- `Cargo.toml` sets `package.metadata.rust-analyzer.rustc_private = true` so
  the IDE can see rustc's crates.

## Why

- **Pinning** turns "breaks at random" into "breaks only when we choose to
  upgrade". Upgrading becomes one deliberate change: bump the date, fix the
  compile errors, run the tests.
- **Reading the exact source.** The pinned commit also exists in a local
  `rust-lang/rust` clone, so `git show 362211dc2:compiler/...` shows exactly
  the API we compile against. No guessing from docs for a different version.
- **The rpath**: our binary loads `librustc_driver.dylib` at startup. Without
  the rpath you'd have to set `DYLD_LIBRARY_PATH` (or `LD_LIBRARY_PATH`) by
  hand. With it, `./target/debug/rust-js` just runs. rustc then finds its
  sysroot (where `core` lives) from that library's location, so the two
  always agree.

## Alternatives

- **Track the latest nightly**: always current, never stable. Rejected.
- **`rustc_public`** (formerly `stable_mir`), rustc's effort at a stable API:
  it exposes MIR, not THIR, so it doesn't give us what [0002](0002-generate-from-thir.md) needs.

## Consequences

- rust-js needs nightly to *build*. The Rust code it compiles still only has to
  be valid Rust for that compiler version.
- The rpath trick in `build.rs` uses `-Wl,-rpath`, which covers macOS and
  Linux, not Windows.
- Upgrading the pin is a maintenance task we'll repeat. Do it in its own
  commit.
