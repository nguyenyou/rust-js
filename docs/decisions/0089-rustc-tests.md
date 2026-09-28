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
- **How a test fails is kept too:** `rejected`, rust-js's own clear error,
  `crashed`, another compile error such as rustc's panic, or `wrong`, JS
  that ran otherwise. A test listed as rejected that now crashes or answers
  wrongly fails the run, as a failure that got worse. The list is 1,495
  rejected and none crashed or wrong.

## Why

- **It measures what we didn't choose:** at `362211dc2`, on Linux, 1,196 of
  the 2,691 tests in scope pass, each matching native Rust byte for byte;
  1,008 are out of scope, each for a reason it prints, and native Rust
  gives no answer here for 69.
- **It found what no example had.** Its first run found, and this change
  fixes:
  - a shift of a narrower integer by an `i64` or a `u64` threw in JS;
  - a `thread_local!` whose last declaration has no `;` was rejected;
  - a crate that enables `decl_macro` or `stmt_expr_attributes` itself was
    rejected, since rust-js enabled them again;
  - an edition after `--` was rejected, as rust-js set 2024 too.

  And eleven that compiled but answered wrongly, now right or rejected:
  - a derived `PartialOrd` of a fieldless enum ordered by declaration, not
    by discriminant, which may be negative (now right);
  - a clone of a closure that changes what it captured shared its state
    (rejected: a JS function can't be copied);
  - a struct made to end in a `dyn`, `&Fat<dyn Trait>`, kept a field that
    isn't one (rejected; one ending in a slice or a `dyn FnMut` is the same
    value, and works);
  - an `#[eii]` function, declared in an `extern` block, was called as a JS
    global (rejected);
  - a `const` of a std struct was its private fields, `iter::empty()` an
    `[undefined]` and a `Cell` a `Cell` in a `Cell` (rejected, but a `Cell`
    or a `RefCell` is its `{ value }`, new at each use).
- **Every failure has a reason a person can act on:** 1,486 of the 1,495
  are a feature rust-js says it doesn't support yet, and counted, they say
  which to do first: std functions (263), types (207), std trait impls
  (178), statics (156), expressions such as `[x; N]` (139), generic trait
  parameters (119), associated types (108).

## Alternatives

- **A chosen subset of passing tests:** smaller, but says nothing of the
  rest, and a newly passing test goes unnoticed.
- **Reasons written by hand, as GopherJS's are:** better reasons, but not
  for 1,500 tests; the first error is a start, and a hand-written note can
  go beside it.

## Consequences

- **The rustc tests workflow runs them on GitHub,** when it's started: all of
  them, on six machines at once, with a release build of rust-js made once
  for all six, and not at all when the compiler's sources are as a cached
  build's were, in about five minutes, checked as one run, or blessed into a new
  list to download; or only the tests and directories named, each said to
  be as the list says or not. Locally, on macOS, each new binary is checked
  before its first run, which makes a full run an hour or more unless the
  terminal is a developer tool (System Settings › Privacy & Security ›
  Developer Tools).
- rust-js checks programs for `wasm32-unknown-unknown` (ADR 0090), and the
  native binary is the 64-bit machine's: a test that asks the width, as
  `cfg(target_pointer_width)`, asks each its own.
- Before ADR 0090, two tests compiled and answered otherwise,
  `const-negation` and `bitwise-ops-platform`: rustc worked out `usize`
  constants at 64 bits.
- The known failures are a release build's on Linux, as the workflow makes
  them, and as `bun run test:rustc` builds: a debug build's deeper stack
  overflows on a test or two a release build passes.
- Which features a crate enables itself, and whether it registers
  `rust_js`, is read from its root's attributes as rustc configures them,
  after parsing: `#![feature (x)]` with a space is one, a comment or a
  string that says `#![feature(x)]` isn't, and neither is a `cfg_attr`
  whose `cfg` doesn't hold. A substring match, then rustc's lexer, had
  some of these wrong, and the lexer panicked on an unfinished `#![`,
  now rustc's syntax error. Found in review.
- A run of some tests is checked as a whole run is: a listed rejection that
  now crashes or answers wrongly isn't as listed. Found in review.
- **The fifteen that crashed now pass, twelve of them, or are rejected:**
  - a `cfg` rustc doesn't expect, in a crate root, left a warning from
    rust-js's own early look at its `cfg`s that belonged to no item, and
    rustc panicked;
  - a `for<'a>` bound, `T: Named<'a>`, and a method with lifetimes of
    its own, `fn pick<'b>(&self, x: &'b u8)`, panicked in rustc when
    rust-js made their dictionaries (ADR 0049);
  - an `async fn` in a trait panicked where it's now rejected;
  - a chain of twenty enums, each holding the next, took minutes to
    compile: whether a type can be written in JS, is changed in place, or
    needs a clone was asked again for each path to it, and is now asked
    once;
  - a `const` too large for rustc's value tree, 100,000 nodes, was
    rustc's error; it's now rust-js's, that it doesn't support it;
  - a test rust-js takes over two minutes to compile is said to, where it
    had been a crash with no reason.

  A `compile-fail` case in the corpus must say its text in its first
  error, so an error of rustc's own can't come before it unseen.
- **A run is checked, or blessed, only if it's whole.** Each shard writes
  which of how many it is, the compiler it ran (its SHA-256), the
  toolchain's and this checkout's commits, every test there is, and the
  ones it was to run. The merge fails, as incomplete, unless each shard is
  there once, all ran the same compiler, toolchain and source as the
  checkout that merges them, with the same tests, each its share, every
  test has one result, and every known failure is a test. A whole local
  run is checked as one shard of one. Found in review: an empty list of
  results passed, with no tests run.
- **What native Rust gives no answer for is listed too,** in
  `test/rustc-native-failures.txt`: a test it can't build, one that
  doesn't end, or ends otherwise than by exiting 0, on its first run or a
  later one, and one that prints what changes from run to run. It's not a
  test out of scope, which its source says, and it isn't passed over: one
  that isn't listed, as a passing test that no longer builds natively, or
  one that's listed and native Rust now answers, fails the run until it's
  blessed. Found in review: a listed test that became a skip passed unseen.
  Listing them found nine tests whose edition is a range, `2015..2021`,
  which rustc was given as it is; compiletest reads one as half-open and
  runs it at its lowest edition, and so does the runner.
- Judged by how it ended (ADR 0088), a compile that crashes after its
  rejection is a crash: two tests listed as rejected were crashes, and
  are rejections now. A union's field, read from a union constant rust-js
  had rejected, was taken as a struct's (`union-const-codegen.rs`); it's
  rejected too. And a `dyn for<'a> AsStr<'a, 'a>`'s dictionary was asked
  of rustc with its lifetime still bound
  (`any-lifetime-escape-higher-rank.rs`); it's made with it erased, as a
  bound's is.
