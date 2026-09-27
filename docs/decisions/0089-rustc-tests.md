# 0089. rustc's own `run-pass` tests run as JS, with a list of what fails that only shrinks

Status: Accepted. Extends [0088](0088-corpus.md).

## Context

Our tests are programs we wrote, around what we knew rust-js does. What
it gets wrong where no one looked, they can't say. rustc has 3,768
`run-pass` UI tests: small programs that each check a piece of the
language, written by the people who know where its edges are. GopherJS
runs Go's own tests, and Kotlin one corpus on every backend, with every
exclusion listed ([research](../research/compiler-testing.md)).

## Decision

**`bun run test:rustc` runs rustc's `tests/ui` `run-pass` tests, at the
pinned toolchain's commit, as corpus cases (ADR 0088):** each is built
natively, with overflow checks off as rust-js takes Rust, and with
rust-js, and the JS must print what the native binary prints, to stdout
and stderr, and return, under Bun and Node.

- **The tests are fetched, not copied:** `scripts/rustc-tests.ts` checks
  out `tests/ui` alone, shallow and sparse, into `target/`.
- **A test that can't be a single program run as JS is out of scope, and
  says why:** it needs another crate, has revisions, needs flags, threads
  or a subprocess, is for some targets, reads files beside it, or its own
  output changes from run to run (a `HashMap`'s order). The counts of each
  are printed; none is hidden.
- **What rust-js gets wrong is `test/rustc-known-failures.txt`**, a test
  and its first error on each line. A test that isn't listed must pass, and
  one that is must fail: when it passes, the run fails until it's taken off
  (`bun run test:rustc:bless`). So the list only shrinks, and a change that
  breaks a passing test is seen.

## Why

- **It measures what we didn't choose:** at `362211dc2`, 1,181 of the 2,688
  tests in scope pass, each matching native Rust byte for byte; 1,080 are
  out of scope, each for a reason it prints.
- **It found what no example had.** Its first run found, and this change
  fixes:
  - a shift of a narrower integer by an `i64` or a `u64` threw in JS;
  - a `thread_local!` whose last declaration has no `;` was rejected;
  - a crate that enables `decl_macro` or `stmt_expr_attributes` itself was
    rejected, since rust-js enabled them again;
  - an edition after `--` was rejected, as rust-js set 2024 too.
- **Every failure has a reason a person can act on:** 1,469 of the 1,507
  are a feature rust-js says it doesn't support yet, and counted, they say
  which to do first: std functions (261), types (209), std trait impls
  (178), statics (156), expressions such as `[x; N]` (140), generic trait
  parameters (119), associated types (108).

## Alternatives

- **A chosen subset of passing tests:** smaller, but says nothing of the
  rest, and a newly passing test goes unnoticed.
- **Reasons written by hand, as GopherJS's are:** better reasons, but not
  for 1,500 tests; the first error is a start, and a hand-written note can
  go beside it.

## Consequences

- A full run takes minutes on Linux. On macOS, each new binary is checked
  before its first run, which makes it about an hour unless the terminal is
  a developer tool (System Settings › Privacy & Security › Developer Tools).
- rust-js compiles with the host's `cfg`, where `target_pointer_width` is
  64, but a `usize` is 32 bits (ADR 0025): tests that ask are among the
  failures, as a difference to decide on.
- Thirteen tests compile but answer wrongly, and are listed with the rest;
  they're the first to fix: derived `PartialOrd` of enums with explicit
  discriminants, a cloned closure, DST trait objects, externally
  implementable items (`eii`), and a const iterator; `const-negation` and
  `bitwise-ops-platform` are the `usize` difference above.
