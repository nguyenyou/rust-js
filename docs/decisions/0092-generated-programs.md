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
- Text where UTF-8 and UTF-16 part: `String`s and `char`s of accents, `ß`,
  CJK and an emoji, upper- and lower-cased, trimmed, replaced, reversed,
  counted in `char`s and by `split`, compared, pushed to, and padded, as
  `{:>8}` counts `char`s; a `char`'s questions, `as u32` and `from_digit`.
- An enum, `E { A, B(i32), C { x: u8, y: bool } }`, made, compared, and
  `match`ed, its arms binding what the variant holds, some with a guard;
  `matches!` and `if let E::B(n)`.
- A `BTreeMap<u8, i32>`, whose order is its keys', not a `HashMap`'s, which
  changes from run to run natively: `insert`, `remove`, `entry(..)
  .or_insert(0) +=`, `get`, `contains_key`, `len`, `values().sum()`, and
  `for` over it.
- Writes inside expressions, `({ x = a; b })`, where any expression may
  be, and an item written through an index that writes first, `v[{ x =
  a; id(0usize) }] += b`: what's read before and after a write says in
  which order an expression's parts run, which Rust's reference fixes and
  JS's rules differ from, in an assignment's place first. A write becomes
  a statement of its own in JS, which runs where it should, so calls with
  effects too, which stay inside an expression: `note(3, a)` prints its
  tag and gives `a`, and `bump(&mut s)` changes `s.a` and gives it,
  as in `s.a += bump(&mut s)`; an index may be `note`'s, and the value
  written through an index that writes may be what it wrote.
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

**A program that differs is kept, then reduced:** before anything else,
`target/fuzz/` gets the program as generated, `seed-N.original.rs`, and
`seed-N.json`: its seed, the compiler's path and SHA-256, the source's
commit, how it failed and what each run printed. Then statements are taken
away, an `if` or a loop made what's in it, an expression made one of its
parts or a literal, each change kept if the program still fails the same
way and is shorter, so the reducing ends. The same way is the same
signature: the same runtimes differing from native Rust in the same of
stdout, stderr and how it ended, or the same crash, so a reduction can't
drift to another bug. Each smaller program is written to `seed-N.rs` as
it's found, and reducing stops after five minutes, saying it didn't
finish, so a reduction that never ends, or a job stopped, loses nothing.
Found in review. What's left becomes a corpus case.

- `bun test` runs the first 12 seeds, which must compile and match: they
  did, so one rust-js rejects is a regression. `FUZZ_START` and
  `FUZZ_SEEDS` explore others, as many as there's time for, where a program
  rust-js doesn't support is skipped. A big batch runs on GitHub, where it
  isn't slowed by macOS's scan of each new binary (AGENTS.md): the rustc
  tests workflow with `fuzz_seeds` splits it across six machines and keeps
  each reduced program as an artifact.

## Why

- **It found what the written tests hadn't:** seed 39, reduced from 17
  statements to one, printed `{:#x}` of a negative `i64` as `0x-80000000`,
  where Rust prints its 64 bits, `0xffffffff80000000`. It's fixed, and the
  corpus keeps it (`radix_negative.rs`). Seed 79, reduced to one line,
  replaced an empty pattern in `"🦀x"`: JS's `replaceAll` put the
  replacement between the emoji's two UTF-16 units, where Rust puts it at
  each char's boundary; `split("")` differed too (`empty_pattern.rs`).
  And ten of 300 seeds used a binding in a guarded arm, `E::B(n) if n > 0`,
  which rust-js rejected; it's supported now (`guarded_bindings.rs`).
- **A failure is a few lines,** which say what's wrong, rather than a
  program of dozens.

## Consequences

- The generator makes only what's here; each kind of Rust it's taught to
  make, structs or collections next, finds its own bugs.
- Seeds 1,000 to 2,199 ran on GitHub before writes were generated; one
  differed, seed 1476, an index checked before its value
  (`assignment_order.rs`). A review found what no seed could, a value read
  after an index that wrote it, and writes are generated since.
- A seed is a program only for this version of the generator: a change to
  it makes other programs of the same seeds, so a failure is kept as its
  reduced program, not its seed.
