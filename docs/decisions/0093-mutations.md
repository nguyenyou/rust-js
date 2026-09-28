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
| `union-field` | a union's field is read as a struct's, and rust-js panics | `union_const.rs` |
| `dyn-bound-lifetimes` | a `dyn for<'a>` trait's lifetime is left bound, and rustc panics | `higher_ranked_dyn.rs` |
| `begin-panic-payload` | `panic!(5)` before edition 2021 throws a message Rust never shows | `begin_panic_value.rs` |
| `lazy-rhs-statements` | `a \|\| f(&mut y)` runs what `f`'s call needs whether or not `a` decides | `lazy_effects.rs` |
| `while-condition-statements` | a `while` condition's statements run after its test | `lazy_effects.rs` |
| `at-binding-copy` | a binding after `@` reads in place what the binding before it changes | `binding_after_at.rs` |
| `option-some-rest` | `Some(..)` is taken as `None` | `option_rest_pattern.rs` |
| `size-align-swap` | `align_of` is the type's size | `size_of.rs` |
| `array-repeat-shared` | `[x; N]` of what's changed is one object, `N` times | `array_repeat.rs` |
| `never-loop-value` | a `loop` that never ends, used as a value, is rejected | `loop_values.rs` |
| `static-struct-variant` | a static struct's fields are read one place along | `statics.rs` |
| `static-mut-place` | a `static mut` is read and written as its value, not its `{ value }` | `static_mut.rs` |
| `static-mut-reference` | a `&mut` to a `static mut` is allowed | `static_mut_reference.rs` |
| `atomic-fetch-new-value` | an atomic's `fetch_add` gives the new value, not the old | `atomics.rs` |
| `thread-local-storage-static` | std's storage for a `thread_local!` is taken as a static, and rejected | `thread_local_syntax.rs` |
| `auto-trait-impl` | an impl of an auto trait, as `unsafe impl Sync`, is rejected | `marker_traits.rs` |
| `user-deref-impl` | a user `Deref` is rejected | `user_deref.rs` |
| `returned-field-write` | a field of what a call's `&mut` points to can't be written | `user_deref.rs`, `returned_references.rs` |
| `own-pointer-unsize` | a pointer of the crate's own, unsized to a `dyn`, is left as it was | `diagnostics.test.ts` |
| `drop-order` | a scope drops what it owns first first, not last first | `drop_scopes.rs` |
| `drop-after-move` | a moved variable is dropped at the end of its scope too | `drop_scopes.rs` |
| `drop-on-assign` | an assignment doesn't drop the old value | `drop_scopes.rs` |
| `drop-without-finally` | a scope's drops run only when it ends normally | `drop_scopes.rs`, `drop_on_panic.rs` |
| `drop-move-before-operands` | a variable moved into a call is taken as moved before a later operand panics | `drop_operand_panic.rs` |
| `drop-function-in-branch` | a drop function is declared in a branch another call to it isn't in | `drop_functions.rs` |
| `drops-walk-uncached` | what a type drops is found once for each path to it, in exponential time | `nested_generic_types.rs` |
| `closure-stepped-iterators` | a closure's body doesn't find its own stepped iterators | `stepped_nested.rs` |
| `static-mut-shared-reference` | a shared reference to a `static mut` is rejected | `static_mut_shared.rs` |
| `drop-ref-parameter` | a parameter bound by `ref` isn't dropped | `drop_params.rs` |
| `drop-let-value-context` | a `let`'s value is taken as moved whatever its pattern | `drop_params.rs` |
| `drop-deref-temporary` | a temporary dereferenced in place is never dropped | `drop_deref_temporary.rs` |
| `temporary-without-finally` | a statement's temporary isn't dropped as a panic unwinds | `drop_temporary_operands.rs` |
| `temporary-not-extended` | a temporary a `let` keeps alive is dropped at the end of the `let` | `drop_temporaries.rs` |
| `unsize-array-as-dyn` | an array unsized to a slice is taken for a `dyn`, and rejected | `drop_temporaries.rs` |
| `generic-drop-not-given` | a generic function given a value with a destructor isn't given its drop | `drop_generic.rs` |
| `generic-drop-not-passed-on` | a generic function doesn't pass its `dropT` on to another | `drop_generic.rs` |
| `part-move-kept-owned` | a field moved out of a value is dropped with it too | `drop_partial.rs` |
| `pattern-parts-kept-owned` | a part a pattern moves out is dropped with what it's matched against too | `drop_partial.rs` |

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
- **A mutation's tests skip the corpus's snapshots** (`RUST_JS_SNAPSHOTS=ignore`):
  nearly any change to the compiler changes some case's JS, so they'd
  catch every mutation, and say nothing of whether what the JS does is
  checked.

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
- **A mutation is caught only by a test that failed:** the runner ends as
  it does when one does, and names at least one that didn't run out of
  time. One that ran out of time, or whose tests did, was stopped, or
  failed before any test did is inconclusive, which
  fails the run as a survivor does, without saying it's caught; each
  mutation's log is kept in `target/mutants/logs/`. Found in review: every
  failed process had counted as caught. Later, a test that timed out had,
  then one whose `beforeEach` hook did.
