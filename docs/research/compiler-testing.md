# Compiler testing: lessons for rust-js

Research date: 2026-09-27. This is a source review of local checkouts, not a
report that their test suites were executed or passed. Revisions below identify
the inspected snapshots; they are not a claim about the latest upstream state.
Every claim below was re-checked against these revisions; citations are
`project/path:line` within each project's repository.

| Project | Inspected revision | Commit date |
| --- | --- | --- |
| ReScript | `5b00bcf69a8946aaf608bfe9c111f95c55aaffc1` | 2026-09-24 |
| Scala.js | `5cc1be6722317e2ae6d0fea6d9d4066f27239eae` | 2026-09-15 |
| SWC | `18de8de9ef69485cb1f3baf80a688e14a16613db` | 2026-03-22 |
| TypeScript | `1f70213d4922b434345f639b441681e470c7cfc1` | 2026-09-04 |
| GopherJS | `490705b1d6fc7d5bd9202ac41888e146183328eb` | 2026-07-27 |
| Kotlin | `a2921f5e28acb4c17c202719117ba26d2a095071` | 2026-05-21 |
| Emscripten | `57df16125662bd7b8c06234ec8b98872f3c4bba0` | 2026-04-01 |

The TypeScript snapshot uses the native Go compiler under `tsc/`. SWC is an
older local snapshot.

## Conclusion

There is no single universal compiler testing standard. These projects converge
on several practices: executable behavior tests, readable output baselines,
negative tests, compiler configuration matrices, integration tests, and tests of
the installed product. Each answers a different question; no single layer proves
the others correct.

rust-js's hardest question is one only some of them face: does an existing
language keep its meaning on a new runtime? The projects that answer it best
run **the source language's own test corpus** on the new target and track every
exclusion with a reason: GopherJS runs Go's, Kotlin runs one corpus on all its
backends, and Scala.js runs Scala's partest. Scala.js is still the closest
overall model; ReScript is useful for generated JavaScript and tooling; SWC for
execution comparison and downstream validation; TypeScript for organizing a
large corpus; Emscripten for configuration matrices and size budgets.

## Observed practices

### ReScript

- Separates runtime (Mocha) tests, compiler output fixtures, internal OCaml
  unit tests, and build integration tests. Generated JavaScript is checked in;
  CI runs the tests and then `git diff --exit-code tests`, so a changed tracked
  output fails. `git diff` does not see new untracked files; the watcher suite
  checks those separately (`no-new-files`, `snapshots-unchanged`).
- Maintains a named error/warning variant inventory linking diagnostics to
  fixtures, with four states: covered, verified unreachable, reachable without
  a fixture, and reachability unconfirmed. It excludes about 94 inline
  `raise_errorf` diagnostics.
- Checks parser/printer roundtrips after normalization: stable text and stable
  AST after printing, rather than requiring the initial AST to be identical.
- Tests watch behavior: atomic saves, newly added files, configuration changes,
  and dependents invalidated after a failed compilation.
- Installs the candidate package (a per-commit preview build) into a temporary
  directory outside the repository and builds and runs a test project on
  macOS, Linux, and Windows, with npm and pnpm.
- Checks source maps by decoding the VLQ mappings and asserting exact original
  positions of chosen tokens, and runs documentation examples as tests.

Sources: `rescript/CONTRIBUTING.md:295` (suites), `:371` (output fixtures);
`rescript/.github/workflows/ci.yml:197` (diff gate), `:468` (installation);
`rescript/tests/ERROR_VARIANTS.md:20` (states);
`rescript/scripts/test_syntax.sh:110` (roundtrips);
`rescript/rewatch/tests/suite.sh:135` (watch tests and self-checks);
`rescript/tests/build_tests/source_map/input.js:124` (source maps).

Do not infer native OCaml semantic equivalence from these tests. That is not the
same contract as rust-js's native Rust comparison.

### Scala.js

- Compiles shared test sources for both JVM and JavaScript, and both run their
  assertions; some tests are JS-only. This is shared reference-runtime testing,
  not a runner comparing every program's stdout.
- Pins the JVM's environment to what Scala.js assumes: root locale, UTF-8,
  `Etc/GMT`, and `\n` line separators.
- Runs the suite across linker and runtime modes: fast/full optimization,
  optimizer on/off, compliant semantics, module kinds and splitting, ECMAScript
  versions, BigInt-backed `Long`s, and WebAssembly. Jenkins distinguishes quick
  and full matrices.
- Checks output stability: link, relink, and clean-link must be byte-identical.
  Incremental linker tests compare each incremental step with a fresh batch
  link, across changes to reachability and optimizer assumptions.
- Reuses upstream Scala tests (partest) with versioned exclusion lists (about
  1,000 entries for 2.13), grouped under reason headings, some still "TODO
  Investigate". The filter fails if an excluded test does not exist.
- Has separate IR checker, analyzer, optimizer, emitter, and printer tests;
  links against every previous library version (backward compatibility); and
  enforces exact library sizes in bytes.

Sources: `scala-js/project/Build.scala:2214` (shared sources), `:2549`
(JVM environment), `:2505` (stability tasks);
`scala-js/Jenkinsfile:163` (stability run), `:205` (modes), `:595` (quick/full);
`scala-js/linker/shared/src/test/scala/org/scalajs/linker/IncrementalTest.scala:39`;
`scala-js/partest/src/main/scala/scala/tools/partest/scalajs/ScalaJSTestFilter.scala:42`;
`scala-js/linker/shared/src/test/scala/org/scalajs/linker/LibrarySizeTest.scala:74`;
`scala-js/linker/shared/src/test/scala/org/scalajs/linker/BackwardsCompatTest.scala:145`.

For rust-js, borrow configuration, stability, and build-history testing. This
does not imply adding an ES5 backend or an optimizer: our corresponding
downstream modes are modern ES output and the actual bundler's
development/production pipelines.

### SWC

- Uses input/output fixtures, execution tests, `.stderr` diagnostic fixtures,
  and source-map outputs, regenerated with `UPDATE=1`.
- Provides a helper that runs original and transformed JavaScript under Node
  and compares stdout. Minifier execution tests compare the original's output
  with compress-only and compress-plus-mangle builds.
- Imports external parser corpora, including Test262 fixtures, with explicit
  exclusions and reasons. Parser acceptance coverage is not full runtime
  Test262 conformance.
- Runs downstream ecosystem suites against a selected SWC release. With
  `--verify` (off by default), it first runs the unchanged downstream suite,
  distinguishing an existing downstream failure from a regression.

Sources: `swc/crates/swc_ecma_transforms_testing/src/lib.rs:533` (stdout
comparison), `:835` (diagnostics);
`swc/crates/swc_ecma_minifier/tests/exec.rs:163`;
`swc/crates/swc_ecma_parser/tests/test262.rs:24`;
`swc/.github/swc-ecosystem-ci/ecosystem-ci.ts:24` (`--verify`).

Execution comparison proves only the behavior observed by each fixture. A test
must deliberately expose side effects, values, and failure behavior that matter.

### TypeScript

- Separates regression and conformance test runners.
- Uses declarative fixture directives for compiler options, combinations of
  options (`// @strict: true, false`), and multiple virtual files in a single
  test case.
- Maintains distinct baselines for diagnostics, JavaScript, source maps,
  types/symbols, and module-resolution traces, accepted with one command.

Sources: `TypeScript/tsc/internal/testrunner/compiler_runner_test.go:20`
(runners); `TypeScript/tsc/internal/testrunner/compiler_runner.go:148`
(option variations), `:204` (baselines);
`TypeScript/tsc/internal/testrunner/test_case_parser_test.go:14` (virtual files).

For rust-js, this is a corpus-organization lesson. rustc still owns Rust type
checking; we should not independently implement or retest its whole type system.

### GopherJS

- Runs Go's own language tests (`$GOROOT/test`) with a fork of Go's runner,
  comparing each program's output with its `.out` file, and runs the standard
  library's own tests under Node.
- Tracks failures in a checked-in map, each with a category (never terminates,
  unsupported package, low-level runtime difference, not applicable, ...) and
  a description. A listed test that fails is a known failure; **a listed test
  that passes fails the run** ("should be removed from knownFails"), so the
  list only shrinks. Tests that never terminate are never started.
- Measures the size of a reference application's build on every pull request.

Sources: `gopherjs/tests/gorepo/run.go:47` (known failures), `:158`
(categories), `:287` (statuses), `:572` (never started).

### Kotlin

- About 7,400 "box" tests under `compiler/testData/codegen/box/` are shared by
  the JVM, JS, Wasm, and Native backends. Each returns `"OK"`; directives such
  as `// IGNORE_BACKEND: JS_IR` and `// TARGET_BACKEND:` state exceptions, and
  `// FILE:` makes multi-file programs.
- A muted test that passes fails ("Looks like this test can be unmuted").
- JS line-number tests end with `// LINES: 1 1 * 3 ...`: the source line of
  each emitted JS statement, checked exactly. Source-map and stepping tests are
  separate.
- 162 incremental-compilation tests apply scripted edits and assert exactly
  which JS files are rebuilt, then run the program after each step.

Sources: `kotlin/compiler/testData/codegen/box/ranges/safeCallRangeTo.kt:1`;
`kotlin/compiler/tests-common-new/testFixtures/org/jetbrains/kotlin/test/backend/BlackBoxCodegenSuppressor.kt:113`;
`kotlin/js/js.translator/testData/lineNumbers/`;
`kotlin/js/js.translator/testData/incremental/invalidation/`.

### Emscripten

- Defines configuration modes in one place (`core0`–`core3`, size modes, LTO,
  `wasm2js`, sanitizers, ...) and runs the same core tests in each; a test opts
  out of a mode with an annotated reason.
- Compares output with checked-in `.out` files, rewritten by `--rebaseline`.
- Checks in exact code sizes (raw and gzip) for canonical programs. Any change,
  including a decrease, fails until rebaselined, so every size change is looked
  at.

Sources: `emscripten/test/test_core.py:9851` (`make_run`), `:9915` (modes);
`emscripten/test/test_codesize.py:146` (size check), `:207` (regression).

## rust-js today

Checked at `5e573ec`.

- **Panics are compared loosely.** The semantics runner
  (`test/semantics.test.ts:37`) turns every JavaScript exception into
  `"panic"`, and its native side drops the message, so a `TypeError` or a
  missing export passes whenever native Rust panics. The main differential test
  uses `toThrow(message)`, which checks a substring and not the error's kind; a
  `TypeError` containing the text passes, and `toThrow("")` accepts any error.
  `test/native.rs` records `""` for a panic payload that isn't a string; none of
  today's 48 panicking cases does, so that hole is latent.
- **Values lose information.** Native results are printed as JSON. `-0` survives
  a direct comparison but not the `JSON.parse(JSON.stringify(..))` applied to
  five examples' results; `NaN` and infinities print as text JSON can't parse,
  so they can't be tested at all; `i64` and `u64` values become JS numbers,
  which lose digits past 2^53.
- **One test hides many.** All ~1,570 native cases are one `test()`, so the first
  mismatch hides the rest, and one compile failure in its setup stops them all.
- **Much of the JS is checked by substring.** About 390 `toContain` assertions
  check exact generated text, much of which the snapshots already cover.
- **One runtime.** Generated programs run under Bun; Node runs only the package
  test, and Chromium only a few browser tests.
- **CI runs only when started by hand.**

Existing foundations to build on: the native-vs-JS differential suite, 42
snapshot crates, ~70 diagnostic cases, seeded numeric tests in
`test/serde.test.ts` and `test/traits.test.ts`, package installation under Bun
and Node, and Chromium tests.

## Recommended next work for rust-js

These are proposals, not features implemented by this research. They are in
order: each makes the next one trustworthy.

### 1. Make the harness strict

A test is only as good as what it can tell apart.

- A Rust panic is a marked JavaScript error carrying its message; the harness
  compares the message exactly, and any other exception fails the case.
- Values are compared through a lossless encoding that tags `-0`, `NaN`,
  infinities, and BigInts, so special floats and 64-bit integers can be tested.
- Each case is its own test.
- Negative controls prove the harness rejects a wrong value, a wrong panic
  message, and a non-panic exception.

### 2. One directive-driven corpus

Move examples and semantic cases into a corpus whose files state what they
expect (`//@ run-pass`, `//@ run-fail: <message>`, `//@ compile-fail`,
`//@ ignore-rust-js: <reason>`), with expected output checked in. This is the
auditable contract: supported semantics, deliberate differences, and compile
rejections, each an executable case. Prioritize feature interactions:
copying with mutation, evaluation order with panics, closure capture with
mutation, and nested representations. A large count of isolated examples is
not a completeness claim.

### 3. Run rustc's own tests

Import pinned `tests/ui` run-pass tests that fit the supported subset, as
GopherJS runs Go's. Track exclusions with a category and reason, fail when an
excluded test passes, and never start tests known not to terminate. The
included fraction is a coverage measure that isn't chosen by us.

### 4. Reuse the corpus across execution paths

Run the corpus under Bun, Node, and a browser, and through the real bundler's
production build with minification. Check exact output sizes of a few canonical
programs. Native and WASM compiler agreement is a consistency check, not an
independent semantic oracle, because they share compiler implementation.

### 5. Test build history and the installed product

Clean builds, warm builds, and edit-then-revert must produce identical output;
once builds are incremental, assert exactly which files are rebuilt. Before
release, install candidate packages outside the checkout and build a small
React REST client under the supported runtimes and operating systems. Existing
package and Vite tests are the foundation; audit them before adding duplicates.

### 6. Generate inputs

Extend the seeded numeric tests into bounded generated Rust programs, with
recorded seeds and reduction of failures to small regression cases. Mutation
testing of the compiler can show which behavior no test observes.

## Execution cost

Keep focused tests convenient during development. A fast tier (harness,
corpus, snapshots, diagnostics) should run automatically on every change, even
if only locally before pushing; the full tier (runtimes, bundlers, browsers,
installation) can run before merging; the platform matrix can be reserved for
release qualification.

No upstream suites or rust-js tests were run as part of this research; the
rust-js observations come from reading its tests and a few matcher experiments
in Bun.
