# 0088. The corpus: Rust programs that say what they expect, run natively and as JS

Status: Accepted. Extends [0017](0017-differential-testing.md).

## Context

ADR 0017 made native Rust the oracle, but how the tests asked it had holes:

- A panic was matched by part of its message, and in the semantics tests
  not at all, so a `TypeError`, or calling a function that isn't there,
  passed for one.
- Values went through JSON, which has no `-0`, `NaN`, infinities or 64-bit
  integers.
- All the native cases were one test, which stopped at the first
  difference.
- Every case was a function the test harness had to know how to call, and
  what rust-js didn't support wasn't written down anywhere a test would
  notice it change.

The compilers that best keep a language's meaning on a new runtime run a
corpus of that language's own programs and track each exclusion
([research](../research/compiler-testing.md)): GopherJS runs Go's, Kotlin
one corpus on every backend.

## Decision

**One oracle, `test/oracle.ts`, compares outcomes exactly:**

- A call ends as `{ value }`, `{ panic: message }` or `{ error }`. A panic
  is a plain `Error` with Rust's whole message (ADR 0012); any other
  exception is an `error`, which no native outcome is.
- Values are compared strictly: `-0` isn't `0`, `1n` isn't `1`. Native
  Rust tags what JSON can't hold: `{"$f64":"NaN"}`, `{"$bigint":"..."}`.
- Each native function is a test, listing every call that differs.
- Negative controls (`test/oracle.test.ts`) show it rejects a wrong
  message, a `TypeError` with the right one, `0` for `-0`, and `1` for `1n`.

**The corpus, `test/corpus/`, is Rust programs, each a `fn main()`** as
rustc's own tests are, run natively and as JS under Bun and Node, and as a
production build ships it: bundled and minified by Vite, with Rolldown and
Oxc, whose names are mangled and whose `new Error(..)` is `Error(..)`, and
run under Node. Each must print the same to stdout and stderr, byte for
byte, and end the same.
What it expects is in a `//@` directive:

| Directive | Means |
|---|---|
| `run-pass`, or none | `main` returns |
| `run-fail: <message>` | `main` panics with exactly this message (`\n` for a newline) |
| `compile-fail: <text>` | rust-js rejects it, with this in its error |
| `ignore-rust-js: <reason>` | rust-js gets it wrong for now |

- **Native Rust checks the directive,** so a case can't expect what Rust
  doesn't do.
- **An ignored case that passes fails:** "remove `ignore-rust-js`". The list
  of what's missing only shrinks, and each entry says why.
- **A case runs in its own process with a time limit,** natively through a
  wrapper that includes it as a module and catches its panic, and as JS
  through `test/corpus-run.ts`, so one that never ends fails alone. A run
  counts only if it exits 0, and only by an outcome it wrote itself: one
  that fails after writing it, as an unhandled rejection after `main`
  makes it, failed, and an outcome left from an earlier run is removed
  first (`test/programs.test.ts`).
- Negative controls show a wrong directive, an ignored case that passes,
  and a `compile-fail` rust-js compiles are each reported.

## Why

- **It finds what the tests we wrote around features didn't.** Its first
  cases found two wrong answers: `grid[0][1] = 5` changed a copy of the row
  and was lost, and `v[f()] += 1` called `f` twice. Both are fixed, with
  cases that keep them so.
- **What rust-js lacks is a list a test keeps:** array repeats `[x; N]`,
  nested `Option`s, `wrapping_neg`, each an `ignore-rust-js` case that fails
  the day it works.
- **A case is plain Rust,** so writing one needs nothing of the harness,
  and rustc's own tests have the same shape (the research's step 3).

## Alternatives

- **Expected output files beside each case:** what rustc's tests check in,
  but native Rust already says what the output is, and can't be wrong
  about it.
- **More functions in `test/native.rs`:** each needs its call written twice,
  in Rust and in the JS test, and says nothing of what's unsupported.

## Consequences

- A case compiles once natively and once with rust-js, under half a second
  each; the corpus grows with that cost.
- Programs that read input, use threads, or need a crate beyond std don't
  fit a case yet.
- A case's `main` returns `()`; `fn main() -> Result` is for later.
- **Every program the tests build or run is judged by how it ended**
  (`test/child.ts`): its exit code, the signal that stopped it, whether it
  ran out of time, 10 seconds to run and 120 to compile, or printed more
  than 16 MB, and what it printed. A compile that failed is rust-js's
  rejection only if it exited 1 with errors of its own; a panic, an
  internal compiler error, a signal, a deadline, another exit code, or an
  error of rustc's is a crash, even after the rejection a `compile-fail`
  case expects, and fails an `ignore-rust-js` case, which rust-js may
  reject or answer wrongly, but not crash on. The corpus, generated
  programs and rustc's tests all
  judge a compile this way. Found in review: a compiler that printed the
  expected rejection, then panicked, passed.
- **The harness's own failures are tested, made on purpose**
  (`test/harness.test.ts`): a compiler that crashes after the rejection a
  case expects, one that never ends, JS wrong under Bun only, a generated
  program's reduction cut short, shards that aren't one run, and a test
  native Rust never ends. Each fails the run, or says it's incomplete,
  with what it saw.
