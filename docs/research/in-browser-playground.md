# Research: an in-browser rust-js playground

Status: **Research note**, not a decision. September 2026. **Spike S1
done; see the update below.** Measurements
use rustc `nightly-2026-03-25` (commit `362211dc2`), the version rust-js
is pinned to.

## Update: spike S1 worked

**rust-js, with rustc's front end, now runs as a WASI program.** Under
wasmtime, `rust-js.wasm` compiles `examples/fib.rs` to JS that is
**byte-identical** to the native build, source map included. Borrow errors
and unsupported features produce rustc's normal diagnostics. The build is
in [`wasm/`](../../wasm/README.md).

Measured, replacing the estimates below:

| | Estimate | Measured |
|---|---|---|
| `rust-js.wasm` | 13–20 MB brotli | **60.6 MB raw, 8.3 MB brotli** |
| Sysroot (only what rustc loads) | 11.5 MB brotli | **15 crates, 56.7 MB raw, 12.8 MB brotli** |
| **First visit** | 25–32 MB | **~21 MB** brotli, plus the WASI shim |
| Compile `fib.rs` | "well under a second" | **~30 ms** (native: 20 ms) |
| Module load, cached / cold | n/a | 0.29 s / 7.7 s (wasmtime's compiler; browsers compile differently) |

How the open questions resolved:

1. **Static linking**: works. We depend on `rustc_driver_impl` directly.
2. **The rustc thread**: `wasm32-wasip1-threads` is a dead end. **wasmtime 49
   removed `wasi-threads` support.** We use `wasm32-wasip1` instead, with a
   15-line patch to run on the current thread. That also removes the need for
   COOP/COEP headers in the browser.
3. **Stack depth**: `stacker` can't find the stack limit on Wasm, and `psm`'s
   precompiled `wasm32.o` gets lost when Apple's `ar` archives it. We fixed
   the archive (use LLVM's `ar`), link a fixed 32 MB stack, and skip stack
   switching on Wasm.
4. **C dependencies**: only `psm`, handled by using LLVM's `ar`.
5. **Build environment**: `RUSTC_BOOTSTRAP=1` plus the `CFG_*` variables.
   `CFG_VERSION` must match the official nightly exactly.

**Five small patches in total** (54 lines, each only affecting Wasm), listed
in [`wasm/README.md`](../../wasm/README.md). Two problems nobody predicted:
the jobserver's helper thread, and a default-sysroot lookup that panics on
WASI even when `--sysroot` is given.

**A detour worth recording.** Warm runs first took 4.2 s, which looked like
a 200× slowdown. It wasn't rustc. Timestamps on WASI calls showed the compile
takes ~30 ms. The remaining 3.7 s is wasmtime *on macOS* unregistering unwind
info for the 60 MB module at exit (seen with `sample`:
`CodeMemory::drop` → `__deregister_frame`). Browsers don't do that.

**Known gaps**: errors end in a trap (exit 134) rather than exit code 1,
because panics can't unwind on `wasm32-wasip1`. This run also didn't isolate
the program's own memory from wasmtime's; S2 will measure it in the browser.

**Next: S2**, the same `.wasm` in a browser via browser_wasi_shim, with no
threads fork or special headers needed now.

## The question

ReScript's playground compiles in your browser: its compiler is itself
compiled to JS. Could rust-js do the same? That means running **rustc's
front end plus rust-js as WebAssembly**, with no server.

The official Rust Playground doesn't do this. It sends your code to a
server, which compiles it in a container.

## Short answer

**Probably yes, and rust-js is unusually well placed for it.** What makes
"rustc in the browser" hard is mostly LLVM, the huge C++ code generator,
and rust-js never uses it. Everything we could check points to a
first-visit download of roughly **25–32 MB** (brotli), then cached. Nothing
found is a blocker. What remains is build plumbing, which a time-boxed spike
can settle.

## What we found

### 1. rustc can already run without LLVM

LLVM is an optional Cargo feature, all the way up the crate graph:

```toml
# compiler/rustc_interface/Cargo.toml
llvm = ['dep:rustc_codegen_llvm']
```

Without it, rustc falls back to a built-in **`dummy` codegen backend**
(`compiler/rustc_interface/src/util.rs`), which is the default when no
backend is configured. It runs the whole front end, produces no machine
code, and can still write `rlib`s containing only metadata.

**Checked, not assumed**: the real rustc with `-Zcodegen-backend=dummy
--target wasm32-unknown-unknown` type-checks `examples/fib.rs`, writes a 4 KB
metadata-only `libfib.rlib`, and still reports type errors normally.

That is exactly rust-js's shape: parse, type check, borrow check, then stop
([ADR 0004](../decisions/0004-driver-hook.md)).

### 2. Upstream already accommodates running rustc on WASI

These are in the pinned source, not patches we'd write:

| Concern | Where | What upstream does |
|---|---|---|
| No `mmap` on Wasm | `rustc_data_structures/src/memmap.rs` | `cfg(target_arch = "wasm32")`: read the file into memory instead |
| Finding the sysroot | `rustc_session/src/filesearch.rs` | WASI stub for `current_dll_path`, so we pass `--sysroot` explicitly |
| File locking | `rustc_data_structures/src/flock.rs` | an `unsupported` fallback for other OSes |
| Ctrl-C handler | `rustc_driver_impl/Cargo.toml` | `ctrlc` excluded on `target_family = "wasm"` |
| Dependencies | `rustc/Cargo.toml` | WASI-specific pins for `getrandom` and `wasi` |
| Job server | `jobserver` crate | ships a `wasm.rs` fallback |

The commit that added the WASI `current_dll_path` (`bdd680ffd15e`,
2025-03-22) calls it "the only change needed to Rust to allow compiling
rustfmt for WASI (rustfmt uses some internal rustc crates)". So rustc's
internal crates already compile to WASI, at least the subset rustfmt uses.

### 3. Prior art: rubrc

[rubrc](https://github.com/oligamiq/rubrc) runs a full rustc, **with**
LLVM, in the browser through WASI, using bjorn3's
[browser_wasi_shim](https://github.com/bjorn3/browser_wasi_shim) plus a
thread-capable fork. It needs cross-origin isolation headers (COOP/COEP)
for `SharedArrayBuffer`. By its own README it's work in progress: no
external crates or proc macros, and occasional errors that break the session.

rubrc proves the hard version works: rustc *with* LLVM, generating
executables. We need the easy version: the front end only.

### 4. Measured sizes

What a browser would download, compressed with brotli (quality 11), as it
would be served:

| Piece | Raw | brotli | Source |
|---|---|---|---|
| `core` metadata, `wasm32-unknown-unknown` | 38.7 MB | 7.8 MB | measured |
| `alloc` metadata | 6.9 MB | 1.9 MB | measured |
| `std` metadata | 6.6 MB | 1.8 MB | measured |
| **Sysroot metadata total** | **52.2 MB** | **11.5 MB** | measured |
| rustc front end + rust-js, as Wasm | ~60–90 MB | **~13–20 MB** | **estimate**: native `librustc_driver` is 81.5 MB (16.9 MB brotli) and includes LLVM's Rust bindings; Wasm size is unmeasured |
| WASI shim + page | < 1 MB | < 1 MB | estimate |
| **First visit** | | **~25–32 MB** | then cached (HTTP cache or service worker) |

For scale, `libLLVM.dylib`, which we **don't** need, is 140 MB raw on its
own. That's more than the whole payload above.

Why `core` is big: its metadata carries MIR for every generic and
`#[inline]` function, for codegen and const evaluation. The front end reads
only what it uses, but it downloads the whole file.

### 5. Speed and memory

- **Native baseline**: rust-js compiles `fib.rs` in **0.02 s** warm (0.28 s
  cold), with a **73 MB** peak resident set. Measured.
- **In the browser (estimate)**: Wasm often runs 1.5–3× slower than
  native, so per-edit recompiles should stay well under a second. The real
  first-load cost is downloading and compiling a ~60–90 MB Wasm module.
- **Memory (estimate)**: without `mmap`, rustc reads all ~52 MB of metadata
  into memory up front. Expect a tab of roughly 200–400 MB.

## What's still unproven

These are what a spike must answer. Each lists what would happen if it fails.

1. **Static linking.** `rustc_driver` is `crate-type = ["dylib"]`, and Wasm
   has no shared libraries. It's only `pub use rustc_driver_impl::*`, so a
   Wasm build should depend on `rustc_driver_impl` directly.
   *If blocked*: a one-line change to the crate type in our build.
2. **The rustc thread.** rustc runs all compilation on a spawned thread
   (`run_in_thread_with_globals`, which ends in `spawn_scoped(..).unwrap()`).
   On `wasm32-wasip1` without threads, that spawn fails. Two ways out:
   - **`wasm32-wasip1-threads`**: works as-is, but the page needs COOP/COEP
     headers, which rules out plain GitHub Pages. A spawn bug on this target
     (rust-lang/rust#146721, a memory-limit issue) was closed on 2026-08-27.
   - **A ~10-line patch** to run inline when threads are unavailable: no
     special headers, simpler hosting. The code's own comment assumes
     `spawn_scoped` "only panics if the thread name contains null bytes",
     which isn't true on thread-less targets, so the patch may be upstreamable.
3. **Stack depth.** rustc grows its stack on demand with `stacker`/`psm`.
   Whether that works on Wasm is **not verified** (the sources weren't in the
   local cache). *If it doesn't*: link with a large fixed stack (e.g. 32 MB).
   That's fine for playground-sized programs.
4. **C dependencies.** With LLVM gone, a few crates might still compile C or
   assembly (for example `psm`, or `blake3` without its `pure` feature).
   *If so*: build them with wasi-sdk, as rubrc does, or pick a pure-Rust
   fallback feature.
5. **Build-time environment.** rustc's crates expect variables that
   bootstrap normally sets (`CFG_RELEASE`, `CFG_VERSION`, ...) and
   `RUSTC_BOOTSTRAP=1`. Tedious rather than hard, but it's where time goes.

**Proc macros** from external crates (like `serde_derive`) can't work,
because loading them means loading a shared library. Built-in derives
(`Clone`, `Debug`, `PartialEq`, ...) live inside rustc and still work.
rust-js doesn't support external crates anyway.

## A useful side effect: pick the target now

In the browser, rust-js would type-check for **`wasm32-unknown-unknown`**,
because that's the sysroot we'd ship. On that target `usize` is **32 bits**,
which fits JS numbers exactly. Using the same `--target` natively too would
keep native and browser builds identical, and would settle the open `usize`
question in [ADR 0011](../decisions/0011-numbers.md). That deserves its own ADR.

## Recommended next step: a time-boxed spike

Keep the browser out of it at first. Prove each layer on its own:

```
S1  rust-js.wasm under wasmtime        (a native WASI runtime: no browser yet)
     build rust-js for wasm32-wasip1-threads, no `llvm` feature,
     link rustc_driver_impl statically, pass --sysroot to the wasm32 metadata
     ✓ done when: `wasmtime run ... rust-js.wasm fib.rs` prints the same fib.js
     → measure the REAL .wasm size (replaces the estimate above)

S2  the same .wasm in a browser        (browser_wasi_shim, threads fork, COOP/COEP)
     ✓ done when: fib.js appears in a page, with timings for download,
       Wasm compile, first rust-js run, and a warm re-run, plus peak memory

S3  a playground page                  (editor → JS + source map → run in a worker)
```

S1 answers every open question except browser hosting. If S1 fails on
something unfixable, we stop early, cheaply, and fall back to a server
playground (run rust-js in a sandboxed container), which needs no research.

## Sources

- rustc source at `362211dc2`: `rustc_interface/{Cargo.toml,src/util.rs}`,
  `rustc_driver{,_impl}/Cargo.toml`, `rustc_data_structures/src/{memmap,flock,stack}.rs`,
  `rustc_session/src/filesearch.rs`, commit `bdd680ffd15e`.
- [rubrc](https://github.com/oligamiq/rubrc): rustc with LLVM in the browser via WASI.
- [browser_wasi_shim](https://github.com/bjorn3/browser_wasi_shim): WASI for browsers.
- [wasm32-wasip1-threads, the rustc book](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip1-threads.html).
- rust-lang/rust#146721: thread spawn on `wasm32-wasip1-threads` (closed 2026-08-27).
- [Running rustc on WASM, Rust Internals (2022)](https://internals.rust-lang.org/t/running-rustc-on-wasm/16198).
