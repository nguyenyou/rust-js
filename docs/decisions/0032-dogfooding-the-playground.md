# 0032. The playground is written in Rust, compiled by rust-js, a part at a time

Status: Accepted.

## Context

The playground (`wasm/web/main.ts`) is a real web app: editors, a file
tree, downloads, a WebAssembly compiler under a WASI shim, and a result
frame. Writing it in Rust with rust-js is the test that matters most: every
gap it hits is one a user would hit. ReScript's own site and tools are
written in ReScript.

Porting all of it at once isn't possible yet. It also uses `try`/`catch`
around a trap, JS `Map`s, a recursive tree (an enum with fields), string and
regex work, and number formatting. The last steps added what the loading part
needs: imports from JS modules (ADR 0028), `async` (0029), `fetch`, binary data
and WebAssembly in the web crate, `Option` (0030) and constants (0031).

## Decision

**The playground moves to Rust a part at a time.** `wasm/web/rust/lib.rs` is
a rust-js crate, and `main.ts` imports what it exports, as it would import
any module:

```ts
import { compile, load, mb, ms, stat } from "./rust/lib.js";
```

The first part is loading: downloading the compiler, the sysroot, the web
crate and the examples, and the stats table. The second is `compile`: running
rust-js.wasm on a crate under the WASI shim, building its directories from the
editor's files and reading the JS back, with a trapped compile as an `Err`
(ADR 0035). It needed `instanceof` in bindings, and `split_once`.

**It's compiled by `rust-js.wasm`**, the compiler the page runs, under the
same WASI shim, in Bun (`wasm/web/compile-rust.ts`). `serve.ts` and
`build.ts` run it before `main.ts` is bundled, so CI needs nothing new. It
takes half a second. The output, `rust/lib.js`, sits beside the source, as
ReScript's does, and isn't committed. The suite also compiles it with the
native rust-js, so a change that breaks the playground fails there first.

**Where rust-js lacks something, the port waits for it**, rather than
working around it in the Rust. Each part moves when rust-js can express it
plainly.

## Why

- **It's the real thing.** It covers JS packages, promises, bytes,
  WebAssembly, the DOM and their interplay, not just examples written to
  pass.
- **One compiler.** The site is built by the same `rust-js.wasm` users run
  in it, so the build tests the deployed compiler, not just the native one.
- **Nothing breaks along the way.** Each part is a separate change, and the
  page works after every one.

## Alternatives

- **Port it all at once**: it would need everything rust-js lacks, first, with
  nothing to show until then.
- **Compile with the native rust-js**: faster to build, but CI would need
  the pinned toolchain's `rustc-dev` and a native build of rust-js, and
  wouldn't exercise the compiler it ships.
- **Commit the generated JS**, as ReScript projects often do: nothing to
  build, but it can go stale, and it doubles every diff.

## Consequences

- `serve.ts` and `build.ts` need `rust-js.wasm` built (they did already).
- TypeScript sees `rust/lib.js` as untyped. `main.ts` states the shape it
  expects where it takes a value from it (`load()`'s result).
- The loading's downloads keep running together, as `Promise.all` had them:
  each starts as it's made, and they're awaited afterwards (ADR 0029).
- What's next to move is what rust-js can express next. A JS `Map` is a type
  in the bindings, with the methods the code uses. Still missing for the rest:
  sorting, iterator adapters, regular expressions, and CodeMirror's API.
