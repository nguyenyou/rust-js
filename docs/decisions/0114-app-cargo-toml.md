# 0114. An app's Rust is a Cargo package, for editors

Status: Accepted. Extends [0105](0105-create.md) and [0113](0113-plain-rustc.md).
Extended by [0115](0115-binding-crates-on-crates-io.md): the crates are packaged
for crates.io; by [0117](0117-output-hooks.md): it has the crate's settings for
its JS.

## Context

A plain rustc compiles a program and the crates it uses (ADR 0113), but an
editor's rust-analyzer finds a crate's dependencies by its `Cargo.toml`. The
vite-react app, which `bun create @rust-js` makes, was `src/App.rs` alone:
to rust-analyzer, `react` was no crate, so nothing in the file resolved,
and no name went to its definition.

## Decision

**The app has a `Cargo.toml`**: `src/App.rs` as its library, and the crates
it uses, `js`, `webapi` and `react`, by path. It's for an editor and a
`cargo check`; the Vite plugin still compiles `App.rs` itself.

- **The paths are where the app installs the crates**: `bun create` rewrites
  the example's, `../../react`, to `node_modules/@rust-js/resources/react`,
  which ships their sources and manifests. The app depends on
  `@rust-js/resources` itself, so every package manager has it there.
- **Each crate, and the app, declares `cfg(rust_js)`** to Cargo, which
  checks the `cfg`s a crate names, and `js::import!` names it in the app.
- **The app ignores `/target` and `/Cargo.lock`**, what a `cargo check`
  writes: the crates are paths, which a lockfile pins nothing of.

## Why

- **rust-analyzer reads Cargo's view of a crate**, and nothing else says
  where `react` is before the crates are on crates.io.
- **The crates the app checks are the ones rust-js builds it with**, the
  installed release's, not another version's.

## Consequences

- **The tests**: a plain `cargo check` of the example, and of an app
  `bun create` made, with the packed `@rust-js/resources` where the app
  installs it: no error, and no warning of the crates.
- **What's used only in JSX is unused to an editor**: react's `jsx!` is a
  placeholder that doesn't look inside the markup (ADR 0113), so a variable
  or static used only there warns, `count` in the app, and a name inside it
  goes to no definition. The tests allow those warnings alone.
