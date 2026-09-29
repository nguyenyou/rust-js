# 0103. The runtime is a package, `@rust-js/runtime`, as ReScript's is

Status: Accepted. Amends [0012](0012-panics-and-runtime-helpers.md) and
[0019](0019-one-js-file-per-module.md), whose helpers are each module's own.

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
- **A module compiled with `--runtime-package` imports the helpers its code
  names,** and defines none: `import { $debugStr, $index } from
  "@rust-js/runtime";`. What those use in turn, the package has.
- **Cargo's builds use it** (ADR 0101): every crate's modules import one
  copy. A build of one file keeps its helpers as ADR 0012 has them, until
  the default changes.
- **A helper's state is the app's now, not each module's:** `$printed`, the
  unfinished line of a `print!` in a browser, is one buffer for every
  module, as Rust has one stdout.

**Where the JS runs, it resolves the package:** an app depends on it, and
the compiler's package will too, as `rescript` depends on
`@rescript/runtime`. In this checkout, the root and the pilot's app depend
on the workspace's. Vite resolves a bare import from a Cargo-built module
from its root (ADR 0101), so the app's is the one.

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
  compiled against the package imports the four helpers it names and runs;
  the pilot's crates import it, and its browser flows pass. Mutations
  catch a module that neither defines nor imports its helpers, and Cargo
  builds that don't use the package.
- **Not yet:** the default for a build of one file, Vite's other mode, the
  corpus and the playground, whose compiled programs run in a page that
  must resolve the package; and refusing a runtime of another version than
  the compiler's, which the manifest's compiler identity (ADR 0042) can.
