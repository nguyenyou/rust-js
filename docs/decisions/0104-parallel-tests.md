# 0104. The tests run side by side, each native program built once

Status: Accepted. Extends [0017](0017-differential-testing.md) and [0088](0088-corpus.md).

## Context

`bun test` took 285 seconds, one file after another. Bun runs test files
side by side with `--parallel`, a worker for each core, and that took 153
seconds and failed 21 tests:

- **Bun's five seconds.** A test that takes three alone takes more beside
  thirteen others.
- **Playwright refused to run.** A worker says it's Jest's
  (`JEST_WORKER_ID`), and Playwright won't run inside Jest.
- **The compiler went missing.** Each file built it, `cargo build`, and
  Cargo links `target/debug/rust-js` again even when nothing's changed,
  so for a moment it isn't there for the other files.

And the workers mostly waited: the CPUs were a seventh busy. A native
program's binary is new each run, and macOS checks each new one as it
first runs, which takes half a second where building it takes a tenth, one
at a time for the whole machine. The corpus, serde's cases and the
generated programs built each of theirs every run.

## Decision

**`bun run test` runs the files side by side** (`bun test --parallel`),
the slowest first, by how long each took last time
(`target/test-timings.json`); `bun run test:serial` runs them one after
another.

- **A test may take a minute** side by side (`test/setup.ts`, loaded
  before each file), and one after another by `--timeout`
  (`test:serial`): Bun loads the setup once then, and sets each file's
  back to its five seconds. A bare `bun test` has those five.
- **The compiler is built once for the run, before Bun starts:** `bun run
  test` and `test:serial` build it, then run Bun with it given,
  `RUST_JS_COMPILER`, so every process of the run has it from the start,
  and nothing builds it again. Set later, by a test file, a child it
  spawns wouldn't have it: Bun gives a child the environment it started
  with. A file that built nothing ran the formatter, and a file its tests
  in a `bun test` of their own, which each built it, and Cargo took it
  away from the files beside them. Found in review.
- **A bare `bun test` builds it too** (`buildCompiler`, from
  `test/setup.ts`, before any file's tests): the first file builds it,
  and the others wait for it. The build is claimed (`once`), by a
  directory of the run's own, `target/tests-build/<run>`, the run being
  its workers' parent by pid and when it started: made whole, with its
  process's pid in it, and moved into place, which only one can do. It's never taken over: a build that
  failed, or whose process ended, is an error for the others, and so is
  waiting ten minutes, and a run's claim is no other run's.
- **A native program is built once, not once a run** (`nativeBinary`): its
  binary is kept in `target/native-cache/`, named by a hash of rustc's
  version, its flags, its source and where it is, since
  `include!("cases.rs")` is of the file beside it, and the libraries it
  links; and used again while none of what it was built from changes.
  That's also what rustc says the build read, its dep-info: each file an
  `include!`, a `mod` or an `include_str!` reached, however deep, whose
  hash is checked each time, and each variable an `env!` read. A new
  build is kept beside those before, which stay: another file may be
  running one.
- **A wrapper is where its text says** (`contentDirectory`), so the same
  program is the same kept binary in any run: a corpus case's includes
  the case where it is, so what it reads beside it is found as the JS's
  compile finds it, serde's cases are copied beside theirs, and a
  generated program is written where its text says. Two files
  building one program write one source, so it's replaced whole, a new
  file moved into place (`writeWhole`), never rewritten as the other's
  rustc reads it. A binary's
  first run may take a minute, not ten seconds: it waits its turn to be
  checked.
- **A file is what runs by itself,** so one taking most of the time is
  split by what it tests: `crates.test.ts`'s Cargo tests are
  `cargo-workspace.test.ts`, `cargo-in-source.test.ts` and
  `cargo-react.test.ts`, sharing `test/crates.ts`.
- **Playwright is run without `JEST_WORKER_ID`.**

## Why

- **The time is the slowest file's, not the sum.** 60 to 70 seconds, with
  the cache warm, and 195 to 210 one after another; a first run, building
  every native program, 142.
- **A binary is the same program each time.** A native program's output
  is the oracle's, and it's run each time; only building it and its first
  check are saved, and a change to anything it's built from builds it again.

## Alternatives

- **Copying a case beside its wrapper,** so a generated program, written
  to a new directory each run, is one binary: it cut what the case reads
  beside it off, which the JS's compile still found. Found in review.
- **A lock, taken over when its holder has ended:** moving a lock aside
  can't tell whether another has just taken it over, and two held it.
  Found in review. A run's own claim is never another run's to take.
- **Replacing a kept binary with a newer build:** another file, given it,
  found it gone. Found in review.
- **Keeping each native program's output, not its binary:** it would save
  running it too, which takes no time, at the risk of an answer the
  program no longer gives.
- **Only the tests a change affects (`bun test --changed`):** Bun follows
  what the tests import, and no test imports the compiler; they run it.
- **Turning macOS's check off for the terminal:** it's the user's security
  setting, and the cache makes it matter only once.

## Consequences

- The files still wait on each other more than they compute: 180 CPU
  seconds take 60 to 70. What's left is mostly Cargo's lock of its package
  cache, and the native programs still built outside `nativeBinary`, a
  few in `crates.test.ts`, `link.test.ts` and `compiler.test.ts`.
- `target/native-cache/`, `target/native-sources/` and
  `target/tests-build/` grow with each program changed and each run; it's in
  `target/`, and removing it only costs building them again.
