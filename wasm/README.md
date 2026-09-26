# rust-js as WebAssembly (spike S1)

This builds rust-js, **together with rustc's front end**, as one WASI
program: `rust-js.wasm`. It compiles Rust to the same JavaScript as the
native build, byte for byte, with no rustc installed where it runs.

It's the first step toward an in-browser playground. See
[docs/research/in-browser-playground.md](../docs/research/in-browser-playground.md).

## Build

You need the pinned toolchain (`rust-toolchain.toml` one level up) and a
rust-lang/rust clone to take rustc's source from:

```bash
git -C <rust clone> worktree add --no-checkout "$PWD/rustc" 362211dc29abc4e8f8cfc384740237f144929b03
git -C rustc sparse-checkout set --cone compiler library/proc_macro
git -C rustc checkout
bun run wasm        # runs ./build.sh
```

`build.sh` applies `patches/`, builds `target/wasm32-wasip1/release/rust-js.wasm`
and stages `sysroot/`, the official `wasm32-unknown-unknown` metadata that
programs are type-checked against.

## Run

With [wasmtime](https://wasmtime.dev):

```bash
wasmtime run --env RUSTC_ICE=0 \
  --dir ../examples::/in --dir out::/out --dir sysroot::/sysroot \
  target/wasm32-wasip1/release/rust-js.wasm \
  /in/fib.rs -o /out/fib.js -- --target wasm32-unknown-unknown --sysroot /sysroot
```

`RUSTC_ICE=0` stops rustc from naming a crash-report file after the process
id, which WASI doesn't have.

## In a browser (spike S2)

`web/` runs the same `rust-js.wasm` in a page, with bjorn3's
[browser_wasi_shim](https://github.com/bjorn3/browser_wasi_shim) providing
WASI on an in-memory filesystem:

```bash
bun run dev     # http://localhost:4400
```

The page compiles `rust-js.wasm` once, downloads the 15 metadata files rustc
needs, and then compiles the crate in the editor on each click (or
⌘/Ctrl-Enter). Each side has a file explorer: the crate's `.rs` files, which
you can add to and delete from, and the JS files it compiles to, one per module
([ADR 0019](../docs/decisions/0019-one-js-file-per-module.md)). The examples
come straight from `examples/`, starting with a counter written against the DOM.
Every program can use the `web` crate (the DOM, [ADR 0024](../docs/decisions/0024-web-crate.md)):
the page downloads its metadata, built for `wasm32-unknown-unknown` by
`web/build.sh`, and passes `--extern web=`.
The Test button compiles with `--test` and runs the crate's `#[test]` functions
in the same frame, in the browser's own DOM ([ADR 0026](../docs/decisions/0026-testing.md)),
with a small stand-in for `bun test`'s `test()`. It needs libtest's metadata too.
If the root module exports `main`, the page runs it after each compile in a
frame with a `<div id="app">`, and the status line says whether it ran. The
modules are linked into one plain script. The frame isn't sandboxed: in some
Chrome setups a sandboxed (out-of-process) frame stays blank until the layout
changes.

**The page is written in Rust**, compiled by rust-js itself
([ADR 0032](../docs/decisions/0032-dogfooding-the-playground.md)):
`web/rust/` is all of it: React components, one per file in `components/` (`App`,
`Toolbar`, `Pane`, `FileTree`, `Editor`, `ResultFrame` and the rest), and the modules they
use (`compiler.rs` runs `rust-js.wasm` under the WASI shim, `codemirror.rs` binds
the editors). `main.ts` only calls `start` in `lib.rs`. The page is a Vite app
([ADR 0045](../docs/decisions/0045-playground-on-vite.md)), with React
([ADR 0044](../docs/decisions/0044-playground-on-react.md)), React Compiler and
Tailwind. `vite-plugin-rust-js` compiles `web/rust/` on start and on each save, with
`rust-js.wasm` under the same WASI shim, in Bun (`web/compile-rust.ts`), so
saving a component's file is a Fast Refresh of that component. On its own: `cd web && bun compile-rust.ts`.

It's also deployed to **https://nguyenyou.github.io/rust-js/** by the
*Deploy playground* workflow (`.github/workflows/deploy-playground.yml`),
which you run by hand from the Actions tab, or with `bun run deploy`.

Compiling rustc's front end takes many minutes on CI, so build locally and
publish the result first. `bun run ship` runs `./build.sh`, then
`./prebuilt.sh publish`, which uploads `rust-js.wasm` as release `wasm-<hash>`,
then starts the workflow:

```bash
bun run ship    # after committing and pushing your changes
```

The hash covers the committed inputs (`src/`, `wasm/Cargo.*`, `.cargo/`,
`patches/`, `build.sh`). The workflow computes the same hash and downloads
the matching binary in seconds. If there isn't one, for example after
changing `src/` without publishing, it builds from source, so a stale binary
is never deployed. `publish` refuses a binary that wasn't built from exactly
the committed, pushed inputs. Once the new one is uploaded, it deletes the
previous `wasm-*` releases (and their tags), so only the newest is kept; an
older commit simply builds from source if deployed. `bun run site` writes the same
static site to `web/dist`, and `bun run preview` serves it under `/rust-js/`,
as Pages does. Each click
gets a fresh instance of the already-compiled module, because rustc keeps
global state and a failed compile ends in a trap.

## How it's put together

- rustc's crates are built from source (`./rustc`, a worktree at the pinned
  commit), because `rustc_private` only ships them for the host. They're
  linked statically: `rustc_driver` is a shared library, which Wasm doesn't
  have, so we depend on `rustc_driver_impl` under the name `rustc_driver`.
- No `llvm` feature: rustc falls back to its built-in `dummy` backend, which
  is the front end only. That's all rust-js needs.
- `Cargo.lock` starts as a copy of rust's own lockfile, so every dependency
  version matches the one rustc was built and tested with.
- `.cargo/config.toml` sets the variables rustc's build system (bootstrap)
  normally provides. `CFG_VERSION` must match the official nightly exactly,
  or rustc refuses the shipped `core`/`std` metadata.
- Target `wasm32-wasip1`, **without threads**: wasmtime 49 removed the
  `wasi-threads` support that `wasm32-wasip1-threads` needs, and a
  thread-less module needs no `SharedArrayBuffer` in a browser. The main
  stack is linked at 32 MB, since rustc normally gets 8 MB on a spawned thread.

## Patches to rustc

Small, and each only changes behavior on Wasm:

| Patch | Why |
|---|---|
| `0001-no-dylib-loading-on-wasm` | Loading proc macros or codegen backends needs shared libraries. On Wasm, fail with a clear error instead of not compiling at all. |
| `0002-run-on-current-thread-without-threads` | rustc runs everything on a spawned thread. Without threads, run on the current one. |
| `0003-wasi-default-sysroot-fallback` | rustc computes a default sysroot even when `--sysroot` is given, and panics on WASI. Fall back to `/sysroot`. |
| `0004-no-jobserver-helper-thread-without-threads` | The jobserver always starts a helper thread. It's only needed for extra compiler threads, which can't exist here. |
| `0005-no-stack-switching-on-wasm` | `stacker` can't find the stack limit on Wasm, so it would allocate a new stack on every call. Use the fixed stack. *Its speed benefit wasn't measured separately.* |

Not a patch, but `build.sh` handles it: `psm` bundles a precompiled
`wasm32.o` with `ar`, and Apple's `ar` silently produces an empty archive.
`build.sh` uses LLVM's `ar` from the toolchain.

## Known gaps

- **Errors exit with 134 (a trap), not 1.** Panics can't unwind on
  `wasm32-wasip1`, and rustc unwinds to bail out after errors. The
  diagnostics are printed first and no JS is written. In a browser, start a
  fresh instance per compile.
- **wasmtime on macOS takes ~3.7 s to exit** after a run, unregistering
  unwind info for the 60 MB module (`CodeMemory::drop` → `__deregister_frame`).
  That's the runtime's cost, not rust-js's. The compile itself takes ~30 ms.
