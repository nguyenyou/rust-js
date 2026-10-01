# 0118. Bindings on npm only, as ReScript's are

Status: Accepted, in place of [0115](0115-binding-crates-on-crates-io.md)'s
crates.io for [0116](0116-binding-versions.md)'s bindings: the community's too
([0119](0119-community-bindings.md)). Proven by `@rust-js/builtins`,
`@rust-js/webapi` and `@rust-js/react`, and
`@rust-js-bindings/canvas-confetti`, an app installing each from npm.

## Context

ADR 0116 versions bindings, and ADR 0115 publishes them to crates.io, a
second registry beside npm, which has the compiler (`@rust-js/native`), its
runtime and resources, and the JS libraries the bindings are for.

ReScript has one registry, npm, for all of it: its compiler, `rescript`,
each platform's binary an optional dependency; its runtime,
`@rescript/runtime`; and every binding, the `.res` source its user's
compiler compiles, `rescript-[library]` by convention, or `@rescript/*`
for the official ones. It can, as each of its tools reads `node_modules`:
`rescript.json`'s `dependencies` are "searched in `node_modules`".

rust-js's tools read Cargo's view of a crate: its own build, a user's
`cargo check`, and rust-analyzer. ADR 0114 already gives Cargo crates from
`node_modules`, by a path in the app's `Cargo.toml`. A path names one
place, which a package manager chooses: a binding that uses another, as
each uses `webapi`, can't name it by path, as where it is depends on npm's
layout, bun's or pnpm's.

Cargo's `[patch]`, in a `.cargo/config.toml`, gives a crate a path for a
name and version wherever it's asked for, and does for a name crates.io has
never had. Tried, with Cargo 1.95: an app and a binding each depending on
`zz-webapi-nowhere-4417 = "1"`, a name no registry has, and the config
patching it to `node_modules/@acme/zz-webapi`, check, offline and online,
with one copy of it, and `cargo metadata`, what rust-analyzer reads, has
it at that path.

## Why npm

**A binding is nothing without its library, which is npm's.** Where
`rust-js-sonner` is used, `sonner` runs, from the app's `node_modules`: an
app that has a binding has a `node_modules`, and the package that's in it.

- **A crate compiled natively too can't use one anyway**: the pilot's
  `models`, shared with a native server, has no JS to call.
- **A Rust library on a binding**, a component library on
  `rust-js-sonner`, ends in an app that installs `sonner`, where the patch
  finds the binding.
- **What crates.io would give, npm has**: a range of the library it binds,
  checked as it's installed, and one install and upgrade of the two.

`js` and `webapi` bind JS and the browser, no npm library, but every app
installs rust-js's packages: they're npm's as they are.

## Decision

**A binding is an npm package, and only one**: its crate, as ADR 0116 has
it, in a package, and Cargo finds it in `node_modules` by a patch rust-js's
tooling writes.

```
 package.json ── npm install ──► node_modules/@rust-js/webapi/{package.json, Cargo.toml, src}
                                 node_modules/@rust-js/builtins/…
                                 node_modules/@rust-js-bindings/canvas-confetti/…
        │
        └─ rust-js-patch, the app's postinstall, by Node's resolution:
           .cargo/config.toml
             [patch.crates-io]
             rust-js-webapi = { path = "node_modules/@rust-js/webapi" }
             rust-js-bindings-canvas-confetti = { path = "node_modules/@rust-js-bindings/canvas-confetti" }

 Cargo.toml (app, and each binding): by name and version, as for crates.io
   webapi = { package = "rust-js-webapi", version = "~0.0.2" }
   canvas_confetti = { package = "rust-js-bindings-canvas-confetti", version = "~1.9" }
```

- **The package is the crate and what npm needs of it**: `Cargo.toml`,
  `src`, and a `package.json` naming the crate,
  `"rust-js": { "crate": "rust-js-bindings-canvas-confetti" }`, with
  `rust-js` among its `keywords`.
- **What a binding shares with the app is a peer dependency**, npm's way of
  saying "the app's copy, not one of mine", as a React plugin has React:

  ```json
  "peerDependencies": {
    "canvas-confetti": ">=1.9.0 <1.10.0",
    "@rust-js/builtins": "~0.0.1",
    "@rust-js/webapi": "~0.0.2"
  }
  ```

  - ② **the library**, its range as ADR 0116 has it, checked by the
    package manager as it installs, where ADR 0116 had rust-js check it;
  - ③ **`builtins` and `webapi`**, which the app installs once, beside the
    compiler: no binding brings its own, so one `webapi::Element` is every
    binding's. At 0.0.x, `~0.0.2` is from 0.0.2 to 0.1, to Cargo and to
    npm alike, so a binding written for 0.0.2 takes the app's later 0.0.x;
  - ① **the oldest rust-js** it needs, a peer dependency on
    `@rust-js/build`, is deferred with ADR 0116's ①: no binding says it
    yet.
- **Cargo names a binding by its crate's name and version**, in the app's
  `Cargo.toml` and in each binding's, as it would one of crates.io's: the
  patch says where it is, so no manifest names a path into `node_modules`,
  and none depends on its layout.
- **A binding's two manifests agree**: what its `Cargo.toml` asks of a
  crate, `webapi = "~0.0.2"`, its `package.json` asks of the package,
  `"@rust-js/webapi": "~0.0.2"`. rust-js's own crates' are written from
  their `Cargo.toml` as they're packed (`scripts/package-npm-crates.ts`):
  each crate it depends on, and what `[package.metadata.rust-js] npm`
  names, `react = ">=18.0.0"` for `react`, are its peers. A binding of the
  community's has both written by hand (ADR 0119); a check that they agree
  is deferred.
- **The patch is written from what's installed**: every package in
  `node_modules` with a `"rust-js"` crate, found by Node's resolution from
  the app and from each such package, in npm's and bun's one
  `node_modules`, or pnpm's store. `rust-js-patch`, `@rust-js/build`'s,
  writes it as the app's `postinstall`, which `bun create` gives it, so an
  editor has it once the app is installed, and the Vite plugin again as it
  starts. Two installed versions of one crate, which a patch can't both
  name, are an error that says which packages asked for each: two
  `webapi`s would be two `Element` types, which Cargo would build, and a
  use of one as the other wouldn't compile.
- **An app in Cargo mode has no `@rust-js/resources`**: `builtins`,
  `webapi` and `react` are packages of their own, and ADR 0114's paths are
  patches. A build without Cargo still has them from `@rust-js/resources`.

## Compared

|  | crates.io (ADRs 0115, 0116) | npm only (this) | ReScript |
|---|---|---|---|
| Where a binding is published | crates.io, the library on npm: two registries | npm, with its library | npm |
| How an app gets it | `npm install` the library, a `Cargo.toml` line for the binding | `npm install` the binding, a `Cargo.toml` line | `npm install`, a `rescript.json` line |
| ② the library's range | rust-js checks `[package.metadata.rust-js] npm` against `node_modules` | the package manager, a peer dependency | a peer dependency |
| ① the oldest rust-js | rust-js checks `compiler = "0.4"` | a peer dependency on `@rust-js/build`, deferred | a peer dependency on `rescript` |
| ③ one `webapi` | Cargo's resolution, `webapi = "1"`; `"2"` beside it builds, and fails where the two meet | a peer dependency, installed once, and the patch naming the one there is | the app's copy |
| A crate using a binding without `node_modules` | yes, from crates.io | no, and it has no library to call | — |
| An editor | Cargo resolves, as for any crate | right once `postinstall` has written the patch; wrong between an install that skipped scripts and the next | the editor extension reads `node_modules` |
| What's locked | `Cargo.lock` and the package manager's lock, which can disagree | the package manager's lock; `Cargo.lock` follows the patch | the package manager's lock |
| Found by | crates.io search | npm's, keyword `rust-js`, as ReScript's Package Index is | npm's, keyword `rescript` |
| A name someone else takes on crates.io | refused: the name is ours once published | harmless with the patch; without it, `cargo check` would fetch theirs, so the names are held there, or asked of a registry of rust-js's own name instead | — |
| rust-js's own code | the metadata checks of ADR 0116 ① and ② | the patch writer, and its crates' peers written as they're packed; the checks are npm's | none |

## Why it might not be chosen

- **The editor depends on a generated file**: an install run with
  `--ignore-scripts`, as CI's often are, leaves the patch stale, and
  rust-analyzer resolves the old versions, or none, until the next.
- **Two locks, one derived**: `Cargo.lock` records what the patch pointed
  at, so it changes whenever `node_modules` does, which a Rust user doesn't
  expect.
- **Each binding says what it uses twice**, in `Cargo.toml` and in
  `package.json`, which must agree.
- **The names must be held on crates.io**, unless Cargo asks for them of a
  registry of rust-js's own name, one it never reaches: a `Cargo.toml`
  naming `rust-js-sonner = "~2.1"` fetches crates.io's `rust-js-sonner`
  without the patch, whoever published it.

## Proven

- The patch written from Node's resolution for npm's and bun's one
  `node_modules` and for pnpm's store (`test/patch.test.ts`).
- Two installed versions of one crate refused, naming who asked for each.
- `@rust-js/builtins`, `@rust-js/webapi` and `@rust-js/react` packed with
  their peers from `Cargo.toml` (`test/npm-crates.test.ts`), and published.
- An app from npm: `bun create @rust-js@latest`, its patch written as it
  installs, `@rust-js-bindings/canvas-confetti` added with `bun add` and a
  line in `Cargo.toml`, the patch written again, and the confetti fired in
  a browser, by `vite dev` and `vite build`.

## Not yet proven

- A peer dependency on `@rust-js/webapi` the app's version doesn't meet,
  refused by each package manager as it installs.
- rust-analyzer resolving a binding to its `node_modules` path, and going
  to a definition in it.
- `npm install --ignore-scripts`: what an editor does before the patch is
  written, and what rust-js says. The Vite plugin writes it as it starts.
- Cargo asking a registry of rust-js's own name, patched to `node_modules`,
  and never reaching it: offline, online, and without the patch, an error
  of that registry's, not a crate of crates.io's. The names aren't held on
  crates.io yet.
