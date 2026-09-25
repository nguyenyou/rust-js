# 0012. Panics throw, via runtime helpers emitted on demand

Status: Accepted

## Context

Some Rust operations panic **in every build profile**, not only debug:

- `a / 0` and `a % 0` panic: "attempt to divide by zero" and "attempt to
  calculate the remainder with a divisor of zero";
- `i32::MIN / -1` and `i32::MIN % -1` panic: "attempt to divide with
  overflow" and "attempt to calculate the remainder with overflow".

JS doesn't: `5 / 0` is `Infinity`, and `Infinity | 0` is `0`. That's a quiet,
wrong answer.

rustc inserts these checks in **MIR**, not THIR, so they aren't in the tree
we read. We have to add them back ourselves.

## Decision

- **A panic is `throw new Error(<Rust's exact message>)`.**
- Division and remainder go through tiny **runtime helpers**, `$div(a, b, min)`
  and `$rem(a, b, min)`, which check, throw, or return the raw result. The
  caller still wraps it (`$div(a, b, -2147483648) | 0`). Signed types pass
  their minimum value; unsigned types omit it.
- **A helper is emitted into the module only if that module uses it.**
- **A literal divisor that can't panic stays inline**: `a / 3 | 0`, not
  `$div(a, 3)`. "Can't panic" means non-zero, and not `-1` for signed types.

## Why

- Correctness first: a Rust program that would panic must not silently return
  0 in JS.
- Rust's exact messages keep the behavior recognizable, and let the
  differential test ([0017](0017-differential-testing.md)) compare panic
  messages, not just "it threw".
- Emitting on demand means programs without division carry no runtime. That
  keeps with ReScript's "tiny runtime" spirit.
- Inlining literal divisors keeps the common case readable.

## Alternatives

- **Inline the checks** at each use site: no helper, but each division
  becomes a multi-line block, or an IIFE in expression position.
- **A shared runtime module** (`import { $div } from "rsjs/runtime"`): better
  once there are many helpers and many modules. Today we have two tiny
  helpers and one module.
- **A dedicated `RustPanic` error class**: nicer for catching panics
  separately from JS errors. Easy to add when `panic!` itself is supported.

## Consequences

- Helper names start with `$`, so they can't collide with Rust names
  ([0010](0010-naming-and-scopes.md)).
- Each module that divides carries its own copy of the helpers. When a
  project has several modules, move to a shared runtime module.
