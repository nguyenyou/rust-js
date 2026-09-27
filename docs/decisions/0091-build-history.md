# 0091. What rust-js writes depends only on what it's given

Status: Accepted. Extends [0019](0019-one-js-file-per-module.md).

## Context

A build tool runs rust-js again and again, over what it wrote before: after
each edit, in a directory the last build left, on machines whose checkouts
are in different places. If what it writes depended on any of that, a build
would pass here and fail there, or keep a module's file after the module is
gone, and no one would see why. The research's step 5 asks for the same
promise Scala.js makes of its linker: a build from nothing and a build over
an older one are the same bytes ([research](../research/compiler-testing.md)).

## Decision

**`test/history.test.ts` holds rust-js to three things:**

- **The same input is the same bytes, each time:** every example, built
  three times in fresh processes, is the same JS and the same source maps.
  So nothing in the compiler, such as a `HashMap`'s order, reaches the
  output.
- **Wherever it's built:** a crate of many modules, built from two places,
  one deeper than the other, is the same tree, but for its manifest's paths
  to its sources, which a build tool reads to know what to watch.
- **A build over an older one is a build from nothing:** a crate goes from
  version to version, with modules added, taken away, renamed, nested and
  made inline, and back to where it began. After each, the directory the
  warm build left is the tree a clean build makes, and the program answers
  as that version should; back at the start, it's the first build's bytes.
  A module taken away takes its JS and its map with it (ADR 0019's
  manifest).

## Why

- **It's what makes a build trustworthy:** an output that's the same for
  the same input can be cached, compared and reviewed.
- **It fails when it should.** Built with rust-js's removal of an old
  build's files turned off, the third test fails at the version whose
  modules were taken away.

## Consequences

- A tool's own caches, the metadata the build adapter keeps
  (`tooling/build.js`), are tested for reuse and rebuilding in
  `test/manifest.test.ts`; the Vite plugin's rebuilds in `test/vite.test.ts`.
- Once rust-js builds a crate's modules one by one, the test should say
  which files each edit rebuilds, as Kotlin's do.
