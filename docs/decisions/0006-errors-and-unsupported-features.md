# 0006. Only programs rustc accepts become JS

Status: Accepted

## Context

Two kinds of "no" can happen:

1. The program isn't valid Rust (a type error, a borrow error).
2. The program is valid Rust, but uses something rust-js can't translate yet
   (`u64`, strings, structs, ...).

## Decision

- **Invalid Rust**: rustc reports its normal error, rust-js writes nothing and
  exits with status 1.
- **Unsupported feature**: rust-js reports a real compiler error at the exact
  source location, reading `rust-js does not support <thing> yet`, then
  writes nothing and exits with status 1.
- **All or nothing**: a single error means no output file.
- **How much is reported in one run**, in two passes:
  1. Items first. Every unsupported item kind (a `const`, a method, a `fn`
     inside a module) is reported. If there are any, rust-js stops here without
     looking at function bodies.
  2. Otherwise, every function is lowered, and each reports its **first**
     unsupported feature.

```
error: rust-js does not support values of type `u64` yet
 --> unsup.rs:1:15
```

## Why

- **Soundness is the product.** Rust's promise is "if it compiles, it's safe".
  If we generated JS from a program the borrow checker rejects, we'd be
  handing out that promise for a program that doesn't have it.
- **Never guess.** When we can't translate something faithfully, the honest
  output is an error, not JS that behaves slightly differently.
- **Compiler errors, not panics**: the user sees the same kind of message, in
  the same format, pointing at the same span, as any other Rust error.
- **All or nothing**: half a module, missing a function other code calls, is
  worse than no module.

## Alternatives

- **Emit what we can, skip the rest**: produces JS that fails later, far from
  the cause.
- **Stop at the first error**: simpler, but slow to fix when there are many.
- **Report every error in every function**: most helpful, but after one
  failure, a function's later errors are often consequences of the first. One
  per function is the current compromise.

## Consequences

- Each lowering function returns `Result<_, ErrorGuaranteed>`.
  `ErrorGuaranteed` is rustc's proof type: you can only get one by actually
  emitting an error, so we can't fail silently by accident.
- As features are added, error sites turn into code. The message list is also
  a to-do list.
