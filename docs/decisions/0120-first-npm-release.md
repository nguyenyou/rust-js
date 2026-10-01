# 0120. The first npm release: 0.0.1, for macOS on Apple silicon

Status: Accepted. Extends [0094](0094-qualification.md) and
[0105](0105-create.md).

## Context

A distribution (ADR 0094) is the packages an app installs, packed and
qualified, and `bun create @rust-js` makes an app of them (ADR 0105), each
from local tarballs: nothing was on npm. Each package was `private`, which
npm refuses to publish. And a released app named the plugin and the runtime,
not the compiler: `@rust-js/build`, which the plugin depends on, was to bring
it, and depends on nothing.

`@rust-js/native` is the compiler for one host, the one that packed it, its
`os` and `cpu` that host's. rust-js links rustc's own libraries, so it's
built on the host it's for. Its one user develops on macOS, on Apple silicon.

## Decision

**rust-js's packages are on npm, at 0.0.1, the compiler for `darwin`
`arm64` alone, built on that host, and published by hand.**

- **Each is publishable**: none `private`, each with its license and its
  repository, at the compiler's version, and each of rust-js's it depends on
  at that version. The tests pack a distribution and `@rust-js/create`, and
  check each.
- **The packages**: `@rust-js/runtime`, `@rust-js/resources`,
  `@rust-js/native`, `@rust-js/build`, `@rust-js/vite-plugin` and
  `@rust-js/create`, published in that order, what each depends on first.
  `@rust-js/builtins` and `@rust-js/webapi` are released on their own.
- **An app's compiler is its own**: `bun create @rust-js` names
  `@rust-js/native` in the app's `devDependencies`, as it did a
  distribution's, where the plugin finds it, from the app.
- **One host, the one it's built on**: `@rust-js/native`'s `os` and `cpu`
  are `darwin` and `arm64`, which a package manager on another refuses to
  install. Another host is a package of its own, `@rust-js/native-<os>-<cpu>`,
  which `@rust-js/native` names, each optional, as esbuild's and ReScript's
  are; it comes when a host's user does.
- **Built and qualified on that host**: a release build, its distribution
  packed, and qualified there (ADR 0094), as CI qualifies Linux's.
- **Published by hand**, its owner's two-factor authentication each time,
  until npm's trusted publishing is a workflow's, as it is for
  `@rust-js/builtins` and `@rust-js/webapi`.

## Why

- **0.0.1**, as `@rust-js/builtins` and `@rust-js/webapi` are: a first
  release, which no user's code depends on yet.
- **One host is the one rust-js has a user on**: a release for a host
  nobody runs would be one to keep for no one (ADR 0094's runs cost ten
  times on macOS's runners).
- **The app names its compiler**, as it names its other packages: the
  plugin finds it from the app, which a package manager that doesn't hoist,
  pnpm's, wouldn't give it from `@rust-js/build`'s own dependencies.

## Consequences

- **`bun create @rust-js` works from npm** on that host, and on another
  stops at the install, `@rust-js/native` refused.
- **A release is a version bump**, of the compiler's `Cargo.toml` and each
  package's `package.json`, its runtime generated again, a release build,
  its distribution qualified, and each package published, in order.
- **A release build is `bun run build:release`**, from 0.0.2, as Qualify's
  is: `--remap-path-prefix=$HOME=~`, so the paths in its panic messages, of
  Cargo's and rustup's sources, are `~/.cargo/..`, as Rust's own are
  `/rustc/<commit>/..`, not the building machine's home. 0.0.1's binary has
  them. The rpath `build.rs` gives it, where rustc's library is for
  `target/debug/rust-js`, still names it: a binary run as it's installed,
  by its launcher, is told where the library is.
