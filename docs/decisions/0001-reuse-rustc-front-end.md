# 0001. Reuse rustc's front end

Status: Accepted

## Context

We want Rust source in, readable JavaScript out: output a person could read,
debug and call from JS, the way ReScript output looks.

Rust has no JS target today. It had one, `asmjs-unknown-emscripten`, which
simulated memory as one big byte array. The output was correct but not
readable, and the target was removed in Rust 1.76.

## Decision

Follow ReScript's recipe. ReScript keeps OCaml's type checker and swaps in a
JS back end. rust-js keeps rustc's parser, name resolution, type inference,
trait resolution and borrow checker, and adds a JS back end on top.

## Why

The hardest parts of a Rust compiler are type inference, traits and the
borrow checker. Reusing rustc means:

- we get them for free, and they behave exactly like Rust, because they *are* Rust;
- any program we accept is one rustc accepts;
- users get rustc's own error messages.

Ownership and lifetimes cost nothing at runtime: they're checked, then erased.
JS has a garbage collector, so erasing them is exactly right.

## Alternatives

| Option | Why not |
|---|---|
| **Rust → Wasm** (wasm-bindgen) | Real Rust, but the output is a binary, not readable JS, and every DOM call crosses a JS↔Wasm boundary. |
| **Our own compiler for a Rust-like language** (the Gleam route) | Full control and clean output, but we'd rewrite inference and traits, and end up with "Rust-like", not Rust. |
| **A full rustc codegen backend** (like `rustc_codegen_cranelift`) | That interface hands us monomorphized MIR, which is too low-level for readable output. See [0002](0002-generate-from-thir.md). |
| **Simulated linear memory** (the asm.js approach) | Supports all of Rust, including `unsafe`, but gives up readability, which is the whole point. |

## Consequences

- We support **safe Rust, a subset at a time**. `unsafe`, raw pointers and
  `transmute` need real memory, and JS objects don't have addresses.
- Rust's standard library is built on `unsafe` internally, so `Vec`,
  `String` and friends will need JS-backed reimplementations. That's the same
  job Scala.js did for the Java library.
- We depend on rustc's unstable internal API. See [0003](0003-pin-nightly-toolchain.md).
