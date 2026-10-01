# 0116. How bindings are versioned

Status: Accepted in part: ② a binding's version is its library's, as
`@rust-js-bindings/canvas-confetti` 1.9.0 is; ③ one `builtins` and one
`webapi` in an app, at versions of their own; and bindings written, not
generated. ① and React's crate per React minor are deferred. Amends
[0115](0115-binding-crates-on-crates-io.md); [0118](0118-bindings-on-npm-only.md)
puts npm's peer dependencies where this has rust-js check.

## Context

rust-js will have many bindings, one for each JS library a program uses,
published as crates. Each has three versions to agree with:

```
          ① compiler ⇄ binding          what the attributes mean
 rust-js ───────────────────── rust-js-react
                                   │  ② binding ⇄ JS library
                                   ▼       which of its API it declares
                                 react (npm)
 rust-js-react ──┐
 rust-js-sonner ─┼──► webapi::Element     ③ binding ⇄ binding
 rust-js-motion ─┘    must be one type     the types they share
```

Others answer each differently:

- **TypeScript**: a `.d.ts` is source, which a newer compiler reads, and a
  package can say the oldest it supports. DefinitelyTyped's
  `@types/react@19.2.x` is for `react@19.2`, its patch its own. The DOM's
  types, `lib.dom.d.ts`, come with the compiler: there's one `Element`.
- **ReScript**: a binding is source its user's compiler compiles, with
  `rescript` a peer dependency; `@rescript/react` has a release per React
  (ADR 0043).
- **Scala.js**: a library is compiled IR, its artifact named for the IR's
  major version, `_sjs1_3`. In 1.x, a newer linker reads an older library's
  IR, and an older one refuses a newer's. ScalablyTyped generates facades
  from DefinitelyTyped's, and has its versions.
- **wasm-bindgen**: its CLI must be the version of the crate a program
  links, exactly, or it refuses, however little changed: the pain point
  each upgrade has.

A rust-js binding is Rust source, which the user's rust-js compiles, as a
ReScript one is: no binary format to keep. What it depends on is what its
`rust_js::` attributes mean. Today rust-js knows the three bindings it
ships by their package names, and ignores a `rust_js::` attribute it
doesn't read, which rustc allows of a tool.

## Decision

### ① A binding works with every rust-js from the one it needs

Deferred: no binding says `compiler` yet, and rust-js reads no binding's
metadata. Until then, a binding is written for the rust-js it's released
with.

**The attributes are a language that only grows**, as Scala.js 1.x's IR
does: an attribute rust-js reads is never taken out, and never means
another thing. So a binding compiles with every rust-js since its own, and
never needs an exact version, wasm-bindgen's way.

- **A binding says the oldest rust-js it needs**, as a crate says the
  oldest Rust, `rust-version`:

  ```toml
  [package.metadata.rust-js]
  compiler = "0.4"
  ```

  rust-js refuses one that needs a newer one: "rust-js-sonner 2.1.0 needs
  rust-js 0.4, and this is 0.3".
- **A `rust_js::` attribute rust-js doesn't know is an error**, where it
  was ignored: "`rust_js::getter` isn't an attribute of rust-js 0.3: this
  binding needs a newer rust-js". A binding without `compiler` still fails
  so, not with wrong JS.
- **The metadata is what makes a crate a binding**, not its name: rust-js
  reads it from the crate's `Cargo.toml`, which Cargo gives the directory
  of, `CARGO_MANIFEST_DIR`, for each crate it compiles.

### ② A binding's version is its library's

**`X.Y.P` is for the library's `X.Y`**, as DefinitelyTyped's are:
`rust-js-react` 19.2.P binds React 19.2, and `P` counts the binding's own
releases, not React's patches. A library at 0.x is too, as Cargo and npm
both take its minor releases as breaking.

- **A binding says which of its library it binds**, its package's
  `peerDependencies` (ADR 0118), `"canvas-confetti": ">=1.9.0 <1.10.0"`,
  which the package manager checks against the version the app installs.
  So a binding never declares what the app's library doesn't have, which
  would compile, then fail in the browser (ADR 0043). A crate of rust-js's
  repository says it in its `Cargo.toml`, packed as those peers:

  ```toml
  [package.metadata.rust-js]
  npm = { react = ">=18.0.0", react-dom = ">=18.0.0" }
  ```

- **An app names a binding by its library's minor**, `~1.9`, not `1.9`,
  which Cargo takes as `^1.9` and would move to 1.10's binding. The
  binding's README says the line, and the install, `canvas-confetti@~1.9`
  and its binding's `@~1.9`.
- **Within `X.Y`, a binding only adds**: a patch release fixes a
  declaration, or adds one the library had, and breaks no program that
  compiled.

**React's binding is rust-js's, at the compiler's version, for every React
from 18.0**: `@rust-js/react` 0.0.2, its peers `react >=18.0.0` and
`react-dom >=18.0.0`. ADR 0043's gates are in its source,
`#[cfg(react = "19.2")]`, and its build script sets them for the app's
React, `RUST_JS_REACT`, which a Cargo build sets from the React the app
has installed, and the latest it knows without it, as rust-analyzer has
it. One crate version per React minor, each with its release's gates set,
is deferred: it's the scheme of every other binding, and no version to
choose by the environment, but a release of each React minor, from 18.0
on, for each of rust-js's.

### ③ `js` and `webapi` are the bindings' standard library

**Every binding shares their types**, so they're one version in any app,
as `lib.dom.d.ts` is: two versions of `webapi` would be two `Element`
types, and one binding's element couldn't be passed to another's.

- **They're 0.0.x, then 1.0, and from 1.0 only ever add**: a binding
  depends on `webapi = "~0.0.2"`, which Cargo and npm both take as below
  0.1, and on `webapi = "1"` from 1.0; their packages are peers (ADR 0118),
  so the app has one copy. What the web platform takes out stays,
  deprecated.
- **They're released with rust-js**, from its repository, with their own
  versions, `builtins` 0.0.1 and `webapi` 0.0.2: a release of rust-js that
  reads a new attribute may come with a release of theirs that uses it.

### Written, not generated

**A binding is written**, as React's is (ADR 0043), by a person or a coding
agent that reads the library's `.d.ts`, its documentation and its source,
not generated from its `.d.ts` as ScalablyTyped's are. What TypeScript
says an overload, a union, `Partial<T>` or `any` is has no one Rust for
it, and a `.d.ts` leaves out what a binding needs, which props are really
optional and what can be `null` (ADR 0043): what's written is run by its
examples, and fixed as it's used (ADR 0119). `webapi` stays generated, from
WebIDL, which says both.

### Where they live

**An official monorepo holds them**, `rust-js-lang/bindings`, as
DefinitelyTyped holds TypeScript's: a crate for each library, checked by
its CI (ADR 0119). The three of rust-js's own, `builtins`, `webapi` and
`react`, stay in its repository, released with it (ADR 0120).

## Why

- **A binding is source, like TypeScript's and ReScript's**: what its user's
  compiler reads is all that must agree, so the attributes keeping their
  meaning is enough. Scala.js keeps its 1.x ecosystem so; wasm-bindgen's
  exact match makes every upgrade break.
- **Mirroring its library's version is what users already know**, from
  DefinitelyTyped: which binding is for which release is its version,
  and the npm range makes Cargo's choice checkable.
- **One binding per React minor, when it comes, is simpler than one for
  all of them**:
  no version to choose by environment, and no build script, and the same
  scheme as every other binding. The gates keep one source for all.
- **Rust makes two versions of a crate two sets of types**, which
  TypeScript's single `lib.dom.d.ts` never has: only a crate that never
  breaks is safe to share.

## Consequences

- **Amends ADR 0115**: a crate's version is its library's, or its own for
  `js` and `webapi`, not the compiler's.
- **Proven**: `@rust-js-bindings/canvas-confetti` 1.9.0, for
  canvas-confetti 1.9, its range a peer dependency, installed by an app
  from npm beside `@rust-js/builtins` 0.0.1 and `@rust-js/webapi` 0.0.2,
  one copy of each.
- **Deferred**: ①, the metadata read in Cargo's build and an unknown
  attribute refused; and one React minor's crate packaged with its gates
  set, which would amend ADR 0043, its release chosen by the binding's
  version, not by `RUST_JS_REACT`.
- **The monorepo** is ADR 0119's.
