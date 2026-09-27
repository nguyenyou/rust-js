# 0092. Generated programs, each from a seed, and reduced when they fail

Status: Accepted. Extends [0088](0088-corpus.md).

## Context

The corpus and rustc's own tests are programs someone wrote, each about
what its writer thought of. What rust-js gets wrong where no one thought to
look, they can't find. The research's step 6 asks for bounded generated
programs, with their seeds recorded and a failure reduced to a small case
([research](../research/compiler-testing.md)).

## Decision

**`test/generate.ts` makes a Rust program from a seed,** the same one each
time, valid and deterministic by construction:

- Integers of every width rust-js has, `i8` to `u64`, and `bool`: literals
  at the edges (0, ±1, the type's bounds, 2^53 + 1), operators, shifts by
  any integer type, casts, wrapping, saturating, `abs`, `count_ones` and
  the like, comparisons, and `if` as an expression; `let`, assignment and
  compound assignment, `if`, bounded `for` loops, and `println!` with `{}`,
  `{:?}`, `{:x}`, `{:#x}` and widths. `usize` isn't generated, as it's a
  known difference (ADR 0090).
- `Vec`s of them: `vec![..]`, `push`, `pop`, `sort`, `reverse`, an item
  read or written, which may not be there and panic, `len`, `contains`,
  `sum`, `max`, and `map` and `filter` with closures; `for` over one.
  `Option`s: `Some`, `None`, `checked_add` and the like, `first`,
  `unwrap_or`, `map`, `is_some`, and `if let Some(x)`. A `Copy` struct of
  three widths, made, compared, read and written. Closures, `move`, of
  what's `Copy`, called.
- It keeps to what the borrow checker allows: a `Vec` is read through
  `clone()`, never moved, and a closure takes copies, so a later write to
  what it captured doesn't conflict with it.
- Each literal is `id(..)`'s, a function rustc doesn't see through, so it
  can't reject a program for an overflow it would work out. A program may
  panic, dividing by zero, as the oracle compares panics too.

**`test/fuzz.test.ts` runs each seed's program as a corpus case is run**
(`test/programs.ts`): natively, and as JS under Bun and Node, which must
print the same and end the same. A program rustc rejects is the
generator's bug and fails the test; one rust-js says it doesn't support is
skipped.

**A program that differs is reduced:** statements are taken away, an `if`
or a loop made what's in it, an expression made one of its parts or a
literal, each change kept if the program still fails the same way and is
shorter, so the reducing ends. What's
left is written with its seed to `target/fuzz/`, to become a corpus case.

- `bun test` runs the first 12 seeds; `FUZZ_START` and `FUZZ_SEEDS` run
  others, as many as there's time for.

## Why

- **It found what the written tests hadn't:** seed 39, reduced from 17
  statements to one, printed `{:#x}` of a negative `i64` as `0x-80000000`,
  where Rust prints its 64 bits, `0xffffffff80000000`. It's fixed, and the
  corpus keeps it (`radix_negative.rs`).
- **A failure is a few lines,** which say what's wrong, rather than a
  program of dozens.

## Consequences

- The generator makes only what's here; each kind of Rust it's taught to
  make, structs or collections next, finds its own bugs.
- A seed is a program only for this version of the generator: a change to
  it makes other programs of the same seeds, so a failure is kept as its
  reduced program, not its seed.
