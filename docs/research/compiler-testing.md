# Compiler testing: lessons for rust-js

Research date: 2026-09-27. This is a source review of local checkouts, not a
report that their test suites were executed or passed. Revisions below identify
the inspected snapshots; they are not a claim about the latest upstream state.

| Project | Inspected revision | Commit date |
| --- | --- | --- |
| ReScript | `5b00bcf69a8946aaf608bfe9c111f95c55aaffc1` | 2026-09-24 |
| Scala.js | `5cc1be6722317e2ae6d0fea6d9d4066f27239eae` | 2026-09-15 |
| SWC | `18de8de9ef69485cb1f3baf80a688e14a16613db` | 2026-03-22 |
| TypeScript | `1f70213d4922b434345f639b441681e470c7cfc1` | 2026-09-04 |

The TypeScript snapshot uses the native Go compiler under `tsc/`. SWC is an
older local snapshot. Links below point to the inspected local source and can
change if those checkouts move to another revision.

## Conclusion

There is no single universal compiler testing standard. These projects converge
on several practices: executable behavior tests, readable output baselines,
negative tests, compiler configuration matrices, integration tests, and tests of
the installed product. Each answers a different question; no single layer proves
the others correct.

Scala.js is the closest model for rust-js because it preserves a source
language's semantics on a different runtime. ReScript is particularly useful for
generated JavaScript and compiler tooling. SWC shows direct execution comparison
and downstream validation. TypeScript shows how to organize a large corpus.

## Observed practices

### ReScript

- Separates runtime execution tests, compiler output fixtures, internal compiler
  unit tests, and build integration tests. Generated JavaScript is checked in;
  CI rejects unexpected changes after running the tests.
- Maintains a named error/warning variant inventory linking diagnostics to
  fixtures. It explicitly distinguishes covered, unreachable, and unfinished
  entries. This inventory does not cover every inline diagnostic.
- Checks parser/printer roundtrips after normalization: stable text and stable
  AST after printing, rather than requiring the initial AST to be identical.
- Tests watch behavior such as atomic saves, newly added files, configuration
  changes, and recovery around failed compilation.
- Installs a candidate package outside the repository and exercises it across
  operating systems. This catches missing distribution files and assumptions
  hidden by a developer checkout.

Sources: [suite guide](/Users/tunguyen/Documents/GitHub/rescript/CONTRIBUTING.md:295),
[runner](/Users/tunguyen/Documents/GitHub/rescript/scripts/test.js:65),
[diagnostic inventory](/Users/tunguyen/Documents/GitHub/rescript/tests/ERROR_VARIANTS.md:1),
[roundtrips](/Users/tunguyen/Documents/GitHub/rescript/scripts/test_syntax.sh:79),
[watch tests](/Users/tunguyen/Documents/GitHub/rescript/rewatch/tests/suite.sh:140),
[installation jobs](/Users/tunguyen/Documents/GitHub/rescript/.github/workflows/ci.yml:469).

Do not infer native OCaml semantic equivalence from these tests. That is not the
same contract as rust-js's native Rust comparison.

### Scala.js

- Compiles shared test sources for both JVM and JavaScript. Both execute the
  same assertions. This is shared reference-runtime testing, not necessarily a
  single runner comparing every program's stdout.
- Explicitly controls environmental assumptions in JVM tests, including locale,
  timezone, encoding, and line separators.
- Exercises different linker and runtime representations: fast/full
  optimization, optimizer enabled/disabled, module formats and splitting, and
  relevant semantic settings. Its Jenkins setup distinguishes quick and full
  matrices.
- Compares output after relinking and clean rebuilding; dedicated incremental
  linker tests exercise changes in reachability and optimization assumptions.
- Reuses upstream Scala tests with versioned exclusions and documented reasons.
  The filter validates that referenced excluded tests exist.
- Has separate IR checker, analyzer, optimizer, emitter, compatibility, and
  library-size tests.

Sources: [shared suite](/Users/tunguyen/Documents/GitHub/scala-js/project/Build.scala:2197),
[JVM environment](/Users/tunguyen/Documents/GitHub/scala-js/project/Build.scala:2542),
[mode matrix](/Users/tunguyen/Documents/GitHub/scala-js/Jenkinsfile:205),
[quick/full matrix](/Users/tunguyen/Documents/GitHub/scala-js/Jenkinsfile:598),
[output stability](/Users/tunguyen/Documents/GitHub/scala-js/project/Build.scala:2507),
[incremental tests](/Users/tunguyen/Documents/GitHub/scala-js/linker/shared/src/test/scala/org/scalajs/linker/IncrementalTest.scala:37),
[upstream test filter](/Users/tunguyen/Documents/GitHub/scala-js/partest/src/main/scala/scala/tools/partest/scalajs/ScalaJSTestFilter.scala),
[explained exclusions](/Users/tunguyen/Documents/GitHub/scala-js/partest-suite/src/test/resources/scala/tools/partest/scalajs/2.13.18/BlacklistedTests.txt:1).

For rust-js, borrow configuration and build-history testing. This does not imply
adding an ES5 backend or an optimizer: our corresponding downstream modes are
modern ES output and the actual bundler's development/production pipelines.

### SWC

- Uses input/output fixtures, execution tests, diagnostic expectations, and
  source-map checks.
- Provides a helper that executes original and transformed JavaScript and
  compares stdout. Minifier execution tests similarly compare before and after
  optimization, including different transformation options.
- Imports external parser corpora, including Test262 fixtures, with explicit
  exclusions. Parser acceptance coverage is not full runtime Test262 conformance.
- Runs downstream ecosystem suites against a selected SWC release. The runner
  can first verify the unchanged downstream baseline, helping distinguish an
  existing downstream failure from a regression introduced by the candidate.

Sources: [execution comparison](/Users/tunguyen/Documents/GitHub/swc/crates/swc_ecma_transforms_testing/src/lib.rs:533),
[minifier execution](/Users/tunguyen/Documents/GitHub/swc/crates/swc_ecma_minifier/tests/exec.rs:163),
[parser corpus](/Users/tunguyen/Documents/GitHub/swc/crates/swc_ecma_parser/tests/test262.rs),
[ecosystem runner](/Users/tunguyen/Documents/GitHub/swc/.github/swc-ecosystem-ci/ecosystem-ci.ts:15),
[downstream example](/Users/tunguyen/Documents/GitHub/swc/.github/swc-ecosystem-ci/tests/swr.ts).

Execution comparison proves only the behavior observed by each fixture. A test
must deliberately expose side effects, values, and failure behavior that matter.

### TypeScript

- Separates regression and conformance test runners.
- Uses declarative fixture directives for compiler options, combinations of
  options, and multiple virtual files in a single test case.
- Maintains distinct baselines for diagnostics, JavaScript, source maps,
  types/symbols, and module-resolution traces.

Sources: [suite entry points](/Users/tunguyen/Documents/GitHub/TypeScript/tsc/internal/testrunner/compiler_runner_test.go:17),
[configuration expansion](/Users/tunguyen/Documents/GitHub/TypeScript/tsc/internal/testrunner/compiler_runner.go:148),
[baseline checks](/Users/tunguyen/Documents/GitHub/TypeScript/tsc/internal/testrunner/compiler_runner.go:366),
[virtual-file fixtures](/Users/tunguyen/Documents/GitHub/TypeScript/tsc/internal/testrunner/test_case_parser_test.go:14).

For rust-js, this is a corpus-organization lesson. rustc still owns Rust type
checking; we should not independently implement or retest its whole type system.

## Recommended next work for rust-js

These are proposals, not features implemented by this research.

### 1. Make the supported contract auditable

Create a small inventory mapping supported semantics and deliberate differences
to executable tests. Use simple fixture metadata for successful execution,
expected panic, compile rejection, and intentional divergence. Record required
environments and the source of the expected behavior.

Reuse existing cases. Prioritize feature interactions: copying with mutation,
evaluation order with panics, closure capture with mutation, and nested
representations. A large count of isolated examples is not a completeness claim.

### 2. Strengthen the existing native oracle

Keep native Rust versus generated JavaScript as the central semantic test.
Compare values and explicit effect traces, and distinguish expected Rust panic
from compiler failure, timeout, and accidental JavaScript exceptions.

In particular, [the current semantics runner](/Users/tunguyen/Documents/GitHub/rust-js/test/semantics.test.ts:35)
maps every JavaScript exception to `"panic"`. For a native case expected to panic,
an unrelated `TypeError` could therefore pass. Add negative controls proving the
harness rejects that mismatch. Preserve special numeric observations explicitly;
plain JSON serialization can erase distinctions such as negative zero and NaN.

### 3. Reuse the corpus across execution paths

Run a representative portable semantic corpus under Bun and Node, then through
production bundling/minification. Extend browser coverage according to the
declared support policy. Existing Node distribution tests and Chromium browser
tests are useful, but do not establish the full semantic corpus works in every
runtime.

Native and WASM compiler agreement is a consistency check, not an independent
semantic oracle, because they share compiler implementation. Keep native Rust
as the reference wherever the language contracts are meant to agree.

### 4. Test build history and the installed product

Compare clean builds with warm builds, edits/reverts, delete/rename operations,
transitive dependency changes, and failure/recovery. Require equivalent behavior
and stable output after accounting for deliberately variable paths or metadata.

Before release, install candidate packages in a directory outside the checkout
and build a small React REST client. Exercise loading/error/empty/success states,
development updates, and production builds. Use the declared supported runtime
versions and operating systems. Existing package/Vite tests are the foundation;
audit their coverage before adding duplicates.

### 5. Expand inputs after the harness is trustworthy

Import selected, pinned upstream rustc tests that fit our supported subset,
maintaining reasons for exclusions. Extend the existing seeded numeric testing
with bounded generated Rust programs, recorded seeds, and reduction of failures
to small regression cases.

Program generation, shrinking, and mutation testing are additional recommendations;
this source review does not establish that all four projects use them or that
they are a universal requirement.

## Execution cost

Keep focused tests convenient during development; provide a full local run for
semantic, integration, browser, and distribution coverage; reserve broader
platform/runtime matrices for manual release qualification. None of this requires
enabling GitHub Actions on pushes or pull requests.

The next highest-value change is the contract inventory plus a shared observation
harness. A new test framework is not necessary. No upstream suites or rust-js
tests were run as part of this documentation-only research.
