# 0103. The runtime is a package, `@rust-js/runtime`, as ReScript's is

Status: Accepted. Replaces [0012](0012-panics-and-runtime-helpers.md)'s helpers,
each module's own, and amends [0019](0019-one-js-file-per-module.md).

## Context

ADR 0012 put each runtime helper into the module that uses it, and named
the alternative, a shared runtime module, as "better once there are many
helpers and many modules". There are 199 now, and the pilot has seven
modules in two crates:

```
frontend/src/api.js     74 KB   the JSON reader and writer, $settle, $debug, ...
models/src/lib.js       71 KB   the JSON reader again
frontend/src/form.jsx    4 KB   $parseInt, $debugStr
frontend/src/route.js    2 KB   $parseInt again
```

A bundler can't merge two copies: to it they're two functions. And the JS
committed beside the Rust (ADR 0041) is mostly helpers, not the program.

ReScript's runtime is a package, `@rescript/runtime`, compiled once by its
team, committed and published at the compiler's version, and a dependency
of `rescript` itself: installing the compiler installs it. Its generated
code imports it:

```js
import * as Primitive_int from "@rescript/runtime/lib/es6/Primitive_int.mjs";
Primitive_int.div(3, 0);
```

## Decision

**The helpers are a package, `@rust-js/runtime`** (`runtime/`), at the
compiler's version, released with it, as ReScript's is:

- **Its module is the compiler's helpers, each exported:** `rust-js
  --runtime-module` prints every helper of `src/runtime.rs`, in its order,
  each top-level `$` name exported. `runtime/index.js` is that, committed,
  and a test checks it's what the compiler prints, and that no two helpers
  declare one name. One source: the compiler.
- **One module, of named exports,** not one per topic as ReScript's is: a
  bundler keeps only what's imported, and one import reads as one.
- **A module imports the helpers its code names,** and defines none:
  `import { $debugStr, $index } from "@rust-js/runtime";`. It asks for
  helpers as it's lowered, as ADR 0012 has it; of those, and of what they
  use, it imports the names its code has. What only another helper uses,
  the package has for it. There's no other way to have them: every build,
  one file's or Cargo's, imports the package, as every ReScript module does.
- **A helper's state is the app's now, not each module's:** `$printed`, the
  unfinished line of a `print!` in a browser, is one buffer for every
  module, as Rust has one stdout.

**Where the JS runs, it resolves the package:**

- **An app depends on it**, as on the compiler: the distribution has
  `runtime.tgz` beside `native.tgz`, of the compiler's version, which the
  distribution checks, as it checks the others.
- **In this checkout,** the root, the playground's app, the examples' and
  the pilot's depend on the workspace's, so JS written anywhere in it
  resolves it: `target/`'s tests, and rustc's, whose CI jobs install it.
- **Vite** resolves it as any package, and a bare import from a Cargo-built
  module from its root (ADR 0101), so the app's is the one.
- **In a browser without a bundler,** an import map names it: the page the
  browser test runner serves (ADR 0027) maps it to the project's installed
  one, and the playground's Result frame to `runtime.js`, which the site
  serves, loaded with the compiler and the crates.

## Why

- **One copy of each helper:** a crate's JSON reader is every crate's.
- **The committed JS is the program:** the pilot's is 17 KB, not 155 KB,
  and `api.js` is its own code and an import.
- **One identity:** a class of the runtime, `$JsonError`, is one class for
  every crate, where ADR 0100 had to keep each crate's to its own.

## Alternatives

- **A runtime generated for each app, of the helpers it uses:** a build
  output of its own to write and commit, and nothing saved, since bundlers
  keep only what's used anyway.
- **One module per topic, as ReScript's:** more import lines for the same
  helpers.
- **Keeping each module's own copy (ADR 0012):** no dependency to install,
  at a copy of every helper in every module.

## Consequences

- **The proof** (`test/runtime-package.test.ts`, and the pilot): a module
  imports the four helpers it names and runs; the pilot's crates import
  it, and its browser flows pass; the corpus, the snapshots and rustc's
  tests run against it. A mutation catches a module that neither defines
  nor imports its helpers.
- **Every generated file changed:** its helpers are an import line. The
  corpus and the snapshots were blessed.
- **A test running JS as a script, not a module,** in a bare VM, puts the
  package's module first, its exports plain declarations.
- **The runtime installed is the compiler's release's:** the build adapter,
  and so Vite's two modes, refuses an app without `@rust-js/runtime`, or
  with another version than the compiler's `--version-json` says, before
  anything's compiled, and says which to install. It's looked for where
  Node finds a package, from the app up, each time: it may be installed
  while a dev server runs. A test checks both refusals.

## Amendment: every package is `@rust-js`'s

The runtime's scope is the project's: each package of this repository is
named in it, as `@vitejs/plugin-react` and `@tailwindcss/vite` are, so an
app's `package.json` says at a glance which of its dependencies are rust-js,
and one npm organization owns every name a release publishes.

| Was | Is |
| --- | --- |
| `vite-plugin-rust-js` | `@rust-js/vite-plugin` |
| `rust-js-build` | `@rust-js/build` |
| `rust-js-native` | `@rust-js/native` |
| `rust-js-resources` | `@rust-js/resources` |
| `rust-js-react-generator`, `rust-js-webapi-generator`, `rust-js-wasm-web` | `@rust-js/react-generator`, `@rust-js/webapi-generator`, `@rust-js/wasm-web` |
| `rust-js`, the repository's root | `@rust-js/workspace` |
| `pilot-web`, `vite-react`, the example apps | `@rust-js/example-pilot-web`, `@rust-js/example-vite-react` |

A distribution's archives are named as the runtime's was, by the package's
name in the scope: `build.tgz`, `vite-plugin.tgz`, `runtime.tgz`,
`resources.tgz` and `native.tgz`. The root and the example apps, which
nothing publishes, are in the scope too, so no name in the workspace is
outside it. The Rust crates (`rust-js-react`, `rust-js-webapi`, and
`rust-js-builtins`, [ADR 0102](0102-js-and-webapi.md)'s) are Cargo's names,
not npm's.
