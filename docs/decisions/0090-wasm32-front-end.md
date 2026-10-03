# 0090. rustc checks programs for `wasm32-unknown-unknown`, whose `usize` is rust-js's

Status: Accepted. Extends [0025](0025-vec-loops-refcell-mut.md).

## Context

rust-js runs rustc's front end, which parses, type-checks and borrow-checks
a program, and works out what depends on the machine it's for: a
constant's value, `size_of`, and which `#[cfg]` holds. It never runs the
back end, which makes machine code.

That machine was the one compiling: a 64-bit one. But a `usize` is 32 bits
in rust-js, as on `wasm32` (ADR 0025), so the two disagreed:

```rust
const M: usize = usize::MAX;     // 18446744073709551615, rounded: 18446744073709552000
let m = usize::MAX;              // the same
std::mem::size_of::<usize>()     // 8
cfg!(target_pointer_width = "32")// false
m.wrapping_add(1)                // wrapped at 2^32 at run time
```

The playground's compiler, running in a browser, already checked programs
for `wasm32-unknown-unknown`, so the same program answered differently
there.

## Decision

**rust-js tells rustc the program is for `wasm32-unknown-unknown`:** a
32-bit `usize`, as rust-js's. So a constant, `size_of`, `cfg` and a
`usize`'s arithmetic all agree, because rustc works them out for the
machine the JS is:

```rust
const M: usize = usize::MAX;     // 4294967295
std::mem::size_of::<usize>()     // 4
cfg!(target_pointer_width = "32")// true
```

- **Nothing is made for WebAssembly.** The target only answers the front
  end's questions; the output is JavaScript, as it was.
- **`cfg(rust_js)` holds,** so a program can tell it's rust-js, as it
  shouldn't have to guess from `target_arch = "wasm32"`.
- **A `--target` after `--` is the one,** as the playground's is.
- **What rust-js reads is built for it too:** the web, React and serde
  metadata, and a Cargo dependency's (`serde/build.sh`, `webapi/build.sh`,
  `react/build.sh`, `tooling/build.js`). A procedural macro, `serde_derive`,
  still runs on the machine compiling, as Cargo builds it.
- **The pinned toolchain installs the target's standard library**
  (`targets` in `rust-toolchain.toml`), so users install nothing more.

## Why

- **One answer for one program:** the native compiler and the playground's
  now compile it the same, and rustc's own constants agree with the JS
  that runs.
- **It's rustc's own answer:** a target it knows, with its standard library
  as it ships, rather than a `usize` rust-js would redefine piece by piece.

## Alternatives

- **A target of rust-js's own** (`js-unknown-unknown`): the most honest
  `cfg`, but rustc has no standard library for it; rust-js would build one.
- **Only `--cfg target_pointer_width="32"`:** rustc doesn't let a built-in
  cfg be set, and a constant would still be 64-bit.
- **A 64-bit `usize`, a BigInt:** as Rust on a server, but every index and
  length would be one, and slow.

## Consequences

- `cfg(target_arch = "wasm32")` and `cfg(target_family = "wasm")` hold, and
  `cfg(unix)` and `cfg(target_os = "linux")` don't. Code for the browser often
  has `wasm32` branches, and gets them; a crate that takes `wasm32` to mean
  wasm-bindgen, as `getrandom` does, may need its JS feature, once rust-js
  compiles dependencies.
- The native tests' oracle is still a 64-bit binary, so a program that
  prints `usize::MAX` differs from it, as ADR 0025 says; a test checks the
  32-bit answers themselves.
- **`size_of::<T>()`, `align_of::<T>()` and a sized value's
  `size_of_val` are the wasm32 target's,** as rustc works them out, as a
  `const` of one already was: a number in the JS. A type parameter's is
  given by the caller (ADR 0145), and an unsized value's is rejected.
  (Amended: a type parameter's was rejected, as a generic function is one
  JS function for every type.) A type with a pointer or a `usize` in it is
  smaller than on a 64-bit machine, as its `usize` is. 27 of rustc's 42
  tests that stopped here pass (`size_of.rs`; a type parameter's in `type_facts.rs`).
