# 0112. rust-js compiles the binding crates

Status: Accepted. Amends [0101](0101-cargo-workspace-wrapper.md), [0109](0109-stable-release.md),
[0110](0110-stable-syntax.md) and [0111](0111-js-types-as-structs.md): no crate a
program uses is unstable.

## Context

The crates a program uses, js, webapi and react, hold declarations, which a
program's compile reads from their metadata: `#[rust_js::link_name]` and the
like, the attributes of rust-js's tool. Plain rustc compiled them, and rustc
knows a tool only a crate registers, `#![register_tool(rust_js)]`, a nightly
feature, which each build script allowed its crate alone (ADR 0109). Under
Cargo (ADR 0101), the toolchain's rustc checked them, with no such allowance,
so on a stable release an app's check failed.

## Decision

**rust-js compiles them, as rustc with its tool known:** `rust-js --rustc
<rustc's flags>` is rustc itself, with `rust_js` a tool it knows and JSX's
syntax, as a program's compile has them (ADR 0110), and nothing of rust-js's
own: it writes what rustc writes, their metadata.

- **Their build scripts run it**, `$RUST_JS_COMPILER`, or this repository's own
  build; a packaged compiler, a JS launcher, with the build's JS runtime. The
  tooling gives its compiler.
- **Cargo's rustc is it**, as the tooling runs Cargo: `$RUSTC` is a shim that
  runs `rust-js --rustc`, the same pinned rustc, for every crate Cargo
  compiles. The binding crates need it whether or not they're the
  workspace's members, which alone the workspace's wrapper runs for: the
  pilot's are paths into this repository. A shell script, for now.
- **So they register no tool, and use no feature**: stable Rust.
- **The playground's deploy builds rust-js natively**, which makes the
  crates the page's Rust uses, as the page runs the WebAssembly one.

## Why

- **rustc keeps a tool's attributes in a crate's metadata** (ADR 0039), and
  a tool rust-js registers is one rustc knows: the one compiler that reads
  the crates' attributes is the one that writes them.

## Consequences

- **No crate a program uses needs a nightly feature**, nor does a program:
  what's unstable is rust-js's own build, which links rustc's internals
  (ADR 0109). The test `the binding crates are stable Rust, which rust-js
  compiles` builds them with no `RUSTC_BOOTSTRAP`.
- **A plain rustc, a user's own `cargo check`, still didn't know the tool**,
  or `jsx!`: ADR 0113 makes a program, and the crates, compile there too.
