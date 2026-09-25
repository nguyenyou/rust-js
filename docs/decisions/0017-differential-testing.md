# 0017. Test against native Rust, not against expectations

Status: Accepted

## Context

How do we know the JS behaves like Rust? Writing expected values by hand
tests *our belief* about Rust. The bugs we most fear, like overflow at a
boundary or a panic that should happen, are exactly where beliefs are wrong.

## Decision

Use **differential testing**: let real Rust be the oracle.

```
examples/fib.rs ──rustc────► native binary ──► expected results ─┐
               └─rust-js───► fib.js       ──► actual results   ─┴─► must be identical
```

- `test/native.rs` includes `examples/fib.rs` as a module, calls each
  function on many inputs, and prints one JSON line per call, either
  `{"value": ..}` or `{"panic": "<message>"}` (via `catch_unwind`).
- It is compiled with **`-Coverflow-checks=off`**: the semantics rust-js
  targets ([0011](0011-numbers.md)).
- `test/fib.test.ts` (run by `bun test`) builds rust-js, generates the JS,
  builds and runs the native binary, then calls the JS with the same inputs.
  It requires equal values and, for panics, a thrown error with Rust's exact
  message.
- Inputs deliberately include the boundaries: u32 wraparound (`fib_iter(47)`
  and beyond), `i32::MAX`/`MIN` through `wrap_demo`, division by zero, and
  `i32::MIN / -1`.
- A second test checks the **source map** ([0018](0018-print-with-oxc.md)).
  `test/sourcemap.ts` is a ~40-line VLQ decoder, so the test needs no
  dependencies. The test decodes the map and requires known JS snippets to
  land on the Rust text that produced them. Its negative control: dropping
  the line shift in `to_oxc.rs` makes it fail.

## Why

- The oracle can't be wrong about Rust, because it *is* Rust.
- One command (`bun test`) runs the whole pipeline end to end: driver,
  lowering, printer and runtime helpers.
- A negative control was checked by hand: dropping the u32 wrapping gives
  `fib_iter(60) = 1548008755920` against Rust's `1820529360`. The test can
  tell the difference.

## Alternatives

- **Snapshot tests of the generated JS**: catch readability regressions, not
  behavior. Worth adding *alongside* this test, not instead of it.
- **Hand-written expected values**: tests our assumptions, not Rust.

## Consequences

- Each new example needs a small native harness entry. Enum arguments are
  mapped by name in the JS test (`"Ascending"`), matching [0013](0013-fieldless-enums.md).
- Tests use `bun`, per project convention. Rust-side tests need nothing
  beyond the pinned toolchain.
