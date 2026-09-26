# 0069. Preserve effects before simplifying; lower once and link afterwards

Status: Accepted. Refines 0042, 0049, 0059 and 0064.

## Context

Combining a map lookup and write into one short JS expression discarded a
required `unwrap` check on overwrite and made `or_insert` arguments lazy.
Rebuilding the seed of `vec![value; n]` bypassed user `Clone`; cloning every
slot ran it once too often. Fieldless enum keys could use custom equality or
ordering that JS Map does not implement.

Lowering also recorded dependencies inside shared `CrateFacts`. To determine
import names it reserved every possible module pair, lowered the crate, then
usually lowered it again. These are output decisions, not immutable facts.

## Decision

Keep the compiler in one crate, with these narrower contracts:

- **Prepare map targets before writing.** `MapPlace` owns an evaluated map
  and key, with any required old value. Preparation emits existence checks
  or entry initialization even when the write overwrites the old value.
  Built-in assignment evaluates its RHS before the target; overloaded
  assignment evaluates the receiver before the argument. `or_insert` is
  eager; `or_insert_with` and `or_default` run factories only for a missing key.
- **Check representation eligibility in `representation.rs`.** Map/set keys
  must have JS-compatible primitive representations and no custom equality;
  ordered collections also reject custom `Ord`. This conservatively rejects
  even compatible hand-written implementations rather than guessing their
  behavior. Derived enum comparisons and primitive keys remain supported.
- **Distinguish structural cloning from user code.** Rebuilding a value is
  allowed only when every owned part has a structural clone. Custom or generic
  clones use `$repeat`: evaluate seed and length once, clone `n - 1` times for
  positive `n`, then move the seed. Length zero still evaluates the seed.
- **Return dependencies with each lowered function.** `CrateFacts` has no
  interior-mutability fields. Each function owns its dependency accumulator,
  including dependencies discovered in closures and copied trait bodies.
- **Lower once, then link.** Cross-module references use temporary symbolic
  identifiers, which cannot collide with source identifiers. After removing
  unused derived Debug implementations, `lower/link.rs` assigns aliases for
  actual dependencies and resolves those identifiers. No unresolved symbol
  may leave lowering. Alias allocation considers nested bindings and variable
  references; locals keep their names and imports receive suffixes as needed.
  Reachability uses adjacency lists rather than repeated scans of all edges.

The JS AST and printer stay independent of rustc identities. Linking is a
small lowering phase; it does not change filesystem publication or the
manifest schema. Snapshots record the intentional alias and map-write changes.

## Validation and scale

`test/semantics.test.ts` compares generated JS directly with native release
Rust, including values, side-effect order, zero/one/many clones and panics.
It also checks unsupported key diagnostics and preservation of previous output.
`test/link.test.ts` covers alias shadowing and sparse module cycles. Existing
trait tests cover copied defaults, cycles, JSX extensions and source locations.

`bun run bench:lowering` compiles sparse cyclic graphs of 10, 100 and 500
modules. Each result is the median of three runs after one warm-up; it includes
rustc, formatting and publication, using the debug compiler. On the development
machine during this change:

| Modules | Before | After |
|---:|---:|---:|
| 10 | 32 ms | 30 ms |
| 100 | 146 ms | 117 ms |
| 500 | 1,934 ms | 1,322 ms |

Temporary instrumentation attributed about 0.68 seconds at 500 modules to
alias reservation and the two lowering passes. Removing that work reduced
end-to-end time by about 32% in this fixture. This is not a production-workload
or release-build guarantee. Rustc still analyzes the whole crate, and output
formatting and file publication still scale with the emitted modules.

## Alternatives

- Add patches to the compact expressions: easy to lose another effect in the
  next operation. Explicit target preparation makes the requirement visible.
- Support arbitrary custom map comparators now: requires a different map
  representation. Reject unsupported keys until that representation exists.
- Split into multiple Cargo crates: adds API work without addressing these
  semantic boundaries.
- Keep two lowering passes: simpler name allocation, but duplicates semantic
  work and creates a quadratic set of speculative module aliases.
