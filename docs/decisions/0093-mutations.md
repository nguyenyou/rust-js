# 0093. Known bugs put back into the compiler, which the tests must catch

Status: Accepted. Extends [0088](0088-corpus.md), [0089](0089-rustc-tests.md) and [0092](0092-generated-programs.md).

## Context

A test that passes says the compiler does what the test checks, not that
the test would see it do otherwise. A corpus case can pass because it's
right, or because it no longer reaches the code it was written for. Twice,
generated programs were run against a compiler with a fix taken out, on a
branch made by hand, to see whether they'd find the bug again: once they
didn't, and the generator was taught what it lacked.

## Decision

**`scripts/mutations.ts` puts known bugs back, one at a time, and runs the
tests named for each against the compiler built with it.** A mutation is a
change to one place in the compiler's source, found exactly as it's
written, and what it breaks, as Rust would see it:

| Mutation | Breaks | Caught by |
|---|---|---|
| `element-value-first` | `v[i] = f()` checks `i` before `f` runs | `assignment_order*.rs` |
| `compound-place-read` | `x += g()` reads `x` before `g` changes it | `assignment_order.rs` |
| `i32-wrap`, `u64-wrap` | arithmetic doesn't wrap at the type's width | `wrapping.rs` |
| `index-panic-message` | an index out of bounds panics with another message | `index_out_of_bounds.rs` |
| `copy-on-read` | a `Copy` value read from a place is the place | `copy_mutation.rs` |
| `guard-statements` | a guard's statements don't run | `guard_statements.rs` |
| `crash-after-rejection` | rust-js panics after it says what it doesn't support | `union_const.rs`, `closure_clone.rs` |
| `operand-capture` | an earlier operand runs after a later one's statements | `operand_prerequisites` (`test/semantics.rs`) |

- **The tests must pass as the compiler is, and run at all,** so their
  failing against a mutation is the mutation's doing, **and fail with a
  compiler that compiles nothing,** so they're using the one they're
  given. The runner's first version set `RUST_JS_COMPILER` in its own
  `process.env`, which Bun doesn't give a process it starts: every
  mutation survived, tested against the compiler as it is.
- **A mutation that isn't caught fails the run,** and so does one that no
  longer applies, as the code it changed has moved, or one that doesn't
  build: each must be made to apply again, or its tests made to see it.
- The crate is copied to `target/mutants/` and built there, with a target
  of its own, so the checkout's build isn't touched, and only rust-js is
  built again for each.
- It builds a few native programs for each, which macOS makes slow, so the
  rustc tests workflow runs it on Linux with `mutations` (AGENTS.md).

## Why

- **It's the claim a green run makes, tested:** that each of these bugs,
  back, turns it red.
- **Each is a bug rust-js had, or one a rule keeps out,** so it's what the
  tests were written for, not a change no program would notice.

## Alternatives

- **A mutation tool, such as `cargo-mutants`:** it changes every operator
  and return it finds, most of which the corpus was never meant to cover,
  and each is a build of rust-js; a list of known bugs is small, and says
  what each test is for.
- **Patches:** they stop applying without saying which line moved; a
  change found as it's written says so.

## Consequences

- A mutation names the code it changes as it's written, so a change to
  that code makes it fail to apply, and it's updated with the code.
- The set is a start: a fix worth keeping is worth a mutation, as its
  corpus case is.
