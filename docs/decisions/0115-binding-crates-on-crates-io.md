# 0115. The binding crates are packaged for crates.io

Status: Accepted: packaged and verified, and published to npm, not
crates.io. Extends [0113](0113-plain-rustc.md) and
[0114](0114-app-cargo-toml.md). [0118](0118-bindings-on-npm-only.md) puts
npm in crates.io's place, as a binding is used only where its npm library
is installed: each crate packaged so is an npm package's, and an app's
`Cargo.toml` names it by version. [0116](0116-binding-versions.md) gives
`builtins` and `webapi` versions of their own.

## Context

A plain stable rustc compiles the binding crates, `js`, `webapi` and
`react` (ADR 0113), and an app's `Cargo.toml` finds them by path, where
`@rust-js/resources` installs them (ADR 0114). A Rust project finds a crate
on crates.io, which they weren't: each was `publish = false`, and depended
on the others by path alone.

The compiler can't be a crates.io crate: it depends on oxc by its git tag,
which crates.io refuses, and it's built with rustc's own libraries, which
`cargo install` gives no crate on a stable release. It's npm's, as it is.

## Decision

**The three crates are packaged as crates.io has them**, by
`bun run pack:crates <out-dir>`, and published by
`bun scripts/package-crates.ts --publish`, after `cargo login`.

- **Named `rust-js-builtins`, `rust-js-webapi` and `rust-js-react`**, as
  they are, at the compiler's version: a program still names them `js`,
  `webapi` and `react`, their libraries' names.
- **Packaged together**: the script stages each as git has it in a workspace
  of the three, so Cargo packages a crate's dependencies first, and verifies
  it against what it packaged, in order. The repository keeps them as it
  did, outside any workspace, so `cargo fmt` of rust-js's own isn't theirs.
- **Verified as a user builds them**: each is built from its package alone,
  with the pinned stable rustc, and no `RUSTC_BOOTSTRAP`.
- **Each is its Rust**, and what react's `build.rs` reads, `versions.json`:
  `include` leaves out the tools that build or generate it.
- **A dependency on another is by path and version**, which crates.io
  keeps, and a checkout the path.

## Why

- **The names were free on crates.io**, and a program's code doesn't
  change: `use react::jsx;` is the library's name, whatever the package's.
- **Packaging proves what publishing would upload**, without uploading:
  publishing can't be undone, and is the maintainer's, with their token.

## Consequences

- **The test** packages the three, at the compiler's version, and checks
  each package's files.
- **Still to come**: publishing them; an app's `Cargo.toml` naming them by
  version, not `@rust-js/resources`' path; and rust-js refusing a binding
  crate of another version than its own, whose attributes it may not read.
