# 0094. A distribution is qualified by the suite, run through what it installs

Status: Accepted. Extends [0088](0088-corpus.md) and [0093](0093-mutations.md).
Extended by [0120](0120-first-npm-release.md): a release's distribution is
qualified on the host it's built for.

## Context

`pack:distribution` makes the four packages an app installs: the compiler
and its launcher, the resources it compiles against, the build tooling and
the Vite plugin, with a manifest and SHA-256 checksums. The package tests
install them outside the checkout and build an app with them. Every other
test runs the checkout's own debug build. So what a user would install had
been built and packed, but never run through the tests that say what
rust-js does (ROADMAP M6.1).

## Decision

**`scripts/qualify.ts` qualifies a distribution on the host it's run on:**

1. **Its files are what it says they are:** each is in `SHA256SUMS` with
   the hash it has, and each artifact is the one `distribution.json` names.
2. **It's for this host, and from this commit, unchanged:**
   `distribution.json` says the platform and architecture it was made for,
   and the commit it was made from and whether that checkout had changes
   of its own. The tests run are that commit's.
3. **It's installed as an app installs it:** into a project of its own,
   outside the checkout, from the packages alone, offline, with no
   lifecycle scripts; and the installed compiler says it's the one the
   manifest names.
4. **The suite runs through what's installed:** every test file, with
   `RUST_JS_COMPILER` the installed launcher, as an app runs it, so the
   launcher's finding its toolchain and the release build are what's
   tested; and the package test on the distribution itself
   (`RUST_JS_DISTRIBUTION`), a copy of it, as it damages one of its files.
   The checkout's own tests also import its plugin and resources, so:
5. **A Vite app is built from the distribution alone:** outside the
   checkout, with the four packages from their tarballs, each installed
   file checked to be the tarball's, and Vite and React from the registry at
   the versions the checkout locks; its build must have `App.rs`'s JSX. A
   plugin that throws fails it. Found in review: the plugin, tooling and
   resources the checkout's tests used were the checkout's, not the
   distribution's.
6. **What was run is written down:** `qualification.json`, with the
   commit, the distribution's manifest, the host (platform, architecture,
   OS release, CPU, memory), Bun's, Node's and rustc's versions, and each
   suite's command, compiler, counts, time and log; and what isn't
   qualified yet. It's qualified only if all of it passed.

**The Qualify workflow does this on each GitHub runner named** (`hosts`,
by default `["ubuntu-latest"]`): a release build, the distribution made
from it, and qualified. Each host's distribution and report are kept as
artifacts, the report for 90 days.

## Why

- **What's tested is what's shipped:** the release build, through the
  launcher, installed from the packages, where every other run tests the
  checkout's debug build.
- **A report says what "qualified" meant:** which commit, which host,
  which runtimes, and how many tests, so one can be compared with the next.

## Alternatives

- **Qualify the checkout's build:** that's what `bun test` does already.
- **Only the package test's app:** it proves the packages install and
  build an app, not that the compiler they install is right.

## Consequences

- Which hosts are promised is the support matrix's to say (ROADMAP M1.1);
  this qualifies whichever is named, and says so.
- The WASM compiler isn't part of a distribution yet, so it isn't
  qualified here; the report says so.
- The installed launcher starts Node before the compiler, so the suite is
  slower through it: a test of many builds has a timeout to match.
- `package-compiler.ts` packages only a native binary: a launcher packaged
  as the compiler started itself, again and again, and a first trial of
  this hung on one until it was stopped.
- Vite is the Vite plugin's optional peer: an offline install that leaves
  peers out still wanted Vite's registry entry for a required one, which a
  new machine hasn't cached, and the first qualification on GitHub failed to
  install. Installs here and in the package test use an empty cache of
  their own, as a new machine has, so a warm one can't hide it again.
- **Qualification runs the suite as it is:** it refuses to run with `BLESS`
  set, which would rewrite what snapshots expect, and leaves out of every
  run the settings that choose tests or how long they have (`FUZZ_*`,
  `RUST_JS_COMPILE_TIMEOUT`, `RUST_JS_REQUIRE_WASM`, and `RUST_JS_SNAPSHOTS`,
  which skips the corpus's snapshots), recording what it left
  out and the seeds that ran. The checkout must have no changes when it
  starts, and the suite none when it ends, but the distribution and report.
  Found in review: `BLESS=1` passed a snapshot that didn't match.
