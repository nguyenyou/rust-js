# Roadmap: full-stack Rust with Scala.js-level maturity

**Rust in. Readable JavaScript out.**

Production readiness means a documented application scope that developers can
install, build, debug, deploy, and upgrade reliably, with evidence that the
compiler preserves its promised behavior and produces readable output.

**End goal: make full-stack Rust applications as dependable and practical as
Scala.js makes Scala applications targeting JavaScript.** That means broad
language and library coverage, correctness across feature combinations,
reusable shared crates, mature JavaScript interop, and reliable development
and release tooling. Readable JavaScript remains part of the promise.

The first production milestone is a React browser application with shared Rust
server types and JSON contracts. It proves an initial supported scope. The
long-term tracks below expand that scope toward the end goal; a successful
pilot alone does not establish Scala.js-level completeness.

Native Rust remains the server target. rust-js must make shared models,
validation, and portable business logic usable on the JavaScript side.
Platform-specific code needs explicit boundaries. Broad language support and
crate reuse are central work, even where an early release can defer them.

## Current position

Assessment: substantial compiler and application foundations are implemented;
production readiness has not yet been established.

Baseline reviewed: `e131de1`, 2026-09-27. “Implemented” below means code and
tests exist in this checkout, not that every combination is supported or that
the tests were rerun for this assessment. The latest successful
[Check run inspected](https://github.com/rust-js-lang/rust-js/actions/runs/36274665281)
tested `48caf69`, an earlier commit.

| Foundation | Evidence already in the repository |
| --- | --- |
| rustc checks, lowering, diagnostics, and output preservation | [Compiler entry point](src/main.rs), [diagnostic tests](test/diagnostics.test.ts), [emission tests](test/emission.test.ts) |
| Collections, traits, generics, iterators, async, and common std operations within a defined subset | [Design decisions](docs/README.md), [native comparisons](test/compiler.test.ts), [trait tests](test/traits.test.ts) |
| Readable JS/JSX and source maps | [Snapshots](test/snapshots/), [JSX tests](test/jsx.test.ts), [emission tests](test/emission.test.ts) |
| React, DOM bindings, Vite, and Fast Refresh | [React guide](react/README.md), [Vite tests](test/vite.test.ts), [browser tests](test/browser.test.ts) |
| Serde-compatible JSON writing and reading for supported types | [Writing contract](docs/decisions/0077-serde-json.md), [reading contract](docs/decisions/0078-serde-json-reading.md), [differential tests](test/serde.test.ts) |
| Browser compiler and playground | [WASM build](wasm/README.md), [playground tests](test/playground.test.ts) |
| Automated checks and a compiler scaling benchmark | [Nightly workflow](.github/workflows/check.yml), [benchmark](bench/lowering.ts) |

## Progress at a glance

| Workstream | Current assessment | Next meaningful proof |
| --- | --- | --- |
| Language and std coverage | Substantial subset; important composition gaps | Current conformance inventory, then systematic closure of gaps (M1, M7) |
| Correctness | Differential, diagnostic, and snapshot suites exist | Required CI, generated cases, and feature-interaction coverage (M2, M7) |
| Full-stack code sharing | Serde support exists; general Cargo reuse is a major gap | Independent client/server pilot, then shared crates and dependencies (M3, M8) |
| JavaScript and React interop | Working bindings, JSX, and Vite integration | External application and library compatibility suite (M3, M9) |
| Distribution and upgrades | Development workflow depends on this checkout | Versioned installation and upgrade tests (M4) |
| Performance and tooling | Scaling benchmark and source maps exist; JSX editor support limited | Measured application budgets and supported editor workflow (M5, M9) |
| Production evidence | Release gates remain open | Qualified release, followed by sustained independent adoption (M6, M10) |

## First production release: M1–M6

These are proposed completion criteria. All gates remain open; existing
foundations count toward them but do not establish release readiness alone.

| Gate | Complete when |
| --- | --- |
| M1 — Define the supported product | Compatibility and semantic contracts are explicit and testable. |
| M2 — Verify every change | Required checks test the candidate commit, including the browser compiler where applicable. |
| M3 — Prove a complete application | An independent React app shares models with a native Rust server and passes end-to-end tests. |
| M4 — Deliver an installable toolchain | A fresh project can install a versioned release without depending on this checkout's layout. |
| M5 — Establish operating limits | Performance, debugging, failure recovery, and supported environments have measured evidence. |
| M6 — Qualify a release | The actual release artifacts pass all agreed gates and a pilot upgrade. |

Start with **M1**, enable **M2** early, and use **M3** to identify the feature
work that matters. Packaging and performance work can proceed alongside the
pilot. **M6** completes the first production release; **M7–M10** describe the
remaining endgame. Work on those tracks can start earlier when needed by the
pilot. No delivery dates are assigned yet.

### M1 — Define the supported product

- [ ] **M1.1 — Publish a compatibility matrix.** Name supported host OSes,
  browsers, JavaScript targets, React/Vite versions, Rust toolchain, and
  dependency model. Distinguish supported, experimental, and rejected cases;
  connect supported claims to tests.
- [ ] **M1.2 — Consolidate the semantic contract.** Give users one current
  reference for numeric widths/overflow, text indexing, copying and mutation,
  eager async, panic/error behavior, and JS boundaries. Link to ADRs and tests;
  historical “not yet” notes must not masquerade as current limitations.
- [ ] **M1.3 — Classify the feature gaps below.** For each, choose implement
  before release, supported workaround, or explicit exclusion. The pilot must
  fit those choices without silently changing behavior.

### M2 — Verify every change

- [ ] **M2.1 — Make CI a merge gate.** Run required checks on pull requests
  and main updates, and require successful checks before merging. Include
  formatting, Clippy, Rust tests, differential tests, snapshots, browser tests,
  and the production example build. Today [Check](.github/workflows/check.yml)
  runs manually only; automatic and scheduled runs are deferred to avoid
  GitHub Actions costs during development.
  Branch-protection enforcement still needs repository configuration.
- [ ] **M2.2 — Require fresh native/browser parity.** Build or fetch WASM for
  the exact candidate sources and run parity plus playground behavior tests
  as a release gate. Missing artifacts must fail that gate. Today
  [playground tests](test/playground.test.ts) may skip without WASM locally;
  Check's WASM parity job builds candidate sources and sets
  `RUST_JS_REQUIRE_WASM=1`, making a missing artifact fatal. Qualification of
  the distributed release artifacts remains open.
- [ ] **M2.3 — Expand adversarial regression coverage.** Add reproducible
  generated/property-based cases for supported constructs, effect order,
  aliasing, Unicode, numeric boundaries, and malformed input. Retain minimized
  regressions. Require diagnostics and preserved output for rejected programs.
  Added evidence: [nested lowering regressions](test/semantics.rs) and their
  [native comparisons](test/semantics.test.ts) cover struct updates, effect
  order, and used inner results inside discarded calls. The [oracle](test/oracle.ts) compares panics by their
  whole message and values strictly, with [negative controls](test/oracle.test.ts)
  ([ADR 0088](docs/decisions/0088-corpus.md)). [Generated programs](test/generate.ts)
  of integer arithmetic, casts and control flow, each from a seed, are
  compared with native Rust and reduced when they differ
  ([ADR 0092](docs/decisions/0092-generated-programs.md)). Of 600 run on
  GitHub, one differed: an element assignment checked its index before its
  value ran, now fixed and kept as a [corpus case](test/corpus/assignment_order.rs).
  Programs now write and call functions with effects inside expressions:
  run against a compiler with the assignment-order fixes undone, 2 of 600
  differed and reduced to those bugs. Other kinds of program remain.
  Eleven known bugs, each put back into the compiler, are caught by the
  tests named for them ([ADR 0093](docs/decisions/0093-mutations.md)).

### M3 — Prove a complete application

- [ ] **M3.1 — Establish a supported shared-code build.** Build a separate
  client and native server from the same model source. Define how dependencies,
  features, macros, and compiler metadata are supplied. Demonstrate rebuilds
  after shared-model edits. General Cargo dependency compilation is still
  outside the [module contract](docs/decisions/0019-one-js-file-per-module.md).
  Added evidence: the [shared-source build recipe](tooling/README.md#share-model-source-with-native-rust)
  and [independent-app test](test/shared-code.test.ts) compile common models and
  validation into native Rust and JavaScript, exchange JSON in both directions,
  reject malformed requests, and rebuild after a shared-source edit. The test
  uses subprocess transport; a Cargo shared-crate build remains open.
- [x] **M3.2 — Integrate Serde into application tooling.** The
  [native adapter](tooling/build.js) and Vite accept `bindings: ["react", "serde"]`.
  They build locked Serde dependencies with the pinned toolchain and discover
  metadata through Cargo's structured output, without manual rustc flags.
  [Independent-app tests](test/manifest.test.ts) cover cache reuse, invalidation,
  paths with spaces, source edits and failure preservation;
  [Vite tests](test/vite.test.ts) cover combined React/Serde builds and refresh.
  General Cargo graph support and browser Serde provisioning remain separate.
- [ ] **M3.3 — Deliver a representative pilot.** Exercise routing, forms,
  validation, lists, async loading/errors, cancellation or stale-response
  handling, and at least one external npm component through the supported
  interop path. Test real client/server JSON in both directions, including
  invalid inputs. Keep the app outside this repository's workspace layout.
- [ ] **M3.4 — Close the pilot's compatibility blockers.** Fix required gaps
  with native comparisons and readable-output snapshots. Demonstrate error
  recovery, Fast Refresh, source-level debugging, and a deployed production
  bundle. Record any remaining limitations in the supported contract.

### M4 — Deliver an installable toolchain

- [ ] **M4.1 — Package the compiler and bindings.** Define versioned artifacts,
  toolchain/sysroot requirements, checksums, and installation for each promised
  host. Verify installation and compilation on clean machines.
  Added evidence: `bun run pack:resources <output.tgz>` creates a versioned
  source-resource package; [package tests](test/packages.test.ts) compile React
  and Serde with unpacked host and resource tarballs outside the checkout.
  `bun run pack:compiler <compiler> <output.tgz>` also packages a native binary
  and launcher for the current host. The installed launcher discovers the pinned
  toolchain libraries and passes the package integration test. Clean-machine
  installation, platform qualification, and signing remain open.
  `bun run pack:distribution <compiler> <new-directory>` assembles all four
  packages with a compiler/host manifest and SHA-256 checksums. Tests verify
  checksums with `shasum`, detect corruption, preserve existing bundles, and
  remove staged output after a packaging failure.
  Distribution tests cover Node.js, including native launching and
  React/Serde preparation, with Bun out of reach; the JS and tooling target
  Node ([ADR 0095](docs/decisions/0095-node-runtime.md)).
- [ ] **M4.2 — Decouple Vite from the source checkout.** Ship the plugin and
  binding assets with explicit versions and configuration. The
  [private plugin](vite-plugin/package.json) now declares its versioned
  [build-host dependency](tooling/package.json); the
  [package test](test/packages.test.ts) verifies offline Bun installation and
  frozen-lockfile reuse of local tarballs, then compiles an independent app with
  automatically discovered compiler and resource packages from the app's
  dependencies. Explicit paths take precedence; checkout defaults are a
  development fallback. A fresh
  app must eventually build using only documented installed dependencies.
- [ ] **M4.3 — Provide a reproducible starter and upgrade path.** Document
  create/build/test/deploy commands, expose compiler/toolchain versions in
  diagnostics, define compatibility/versioning rules, and publish migration
  notes for breaking changes. Verify a previous-version app can upgrade and
  roll back using the documented steps.
  Added evidence: `--version-json` reports the manifest compiler identity;
  packaged-resource preparation rejects release/pin mismatches before building.
  [Package tests](test/packages.test.ts) verify rejection, preserved output, and
  recovery after restoring matching resources. A real release upgrade remains open.

### M5 — Establish operating limits

- [ ] **M5.1 — Measure application-scale performance.** Set budgets before
  claiming readiness: cold build, edit-to-refresh, peak memory, generated JS
  size, helper overhead, and runtime hot paths. Record hardware, fixture sizes,
  versions, and repeatable measurements. The existing module-graph benchmark
  is a useful start, but it does not measure a production application.
- [ ] **M5.2 — Validate supported environments.** Run the agreed host/browser
  matrix in CI. Current [browser configuration](browser/playwright.config.ts)
  covers Chromium and Check runs on Ubuntu. Add Firefox/WebKit and other hosts
  if they are in the M1 support promise.
- [ ] **M5.3 — Make everyday debugging practical.** Document and test editor
  setup, format-on-save, compiler diagnostics, and source maps through the
  production bundler. [Stock rust-analyzer does not expand JSX](docs/jsx.md#current-boundaries);
  deliver the editor support required by the pilot and disclose remaining
  limits. Full JSX completion can be scoped separately.
- [ ] **M5.4 — Exercise failure and upgrade recovery.** Test interrupted builds,
  unwritable output, stale metadata, dependency upgrades, and repeated edits.
  Preserve user files and previous output. Audit installed artifacts, runtime
  helpers, and dependencies; document how users report compiler/security bugs.

### M6 — Qualify a release

- [ ] **M6.1 — Test the release artifacts themselves.** Run the complete
  agreed suite and pilot against the exact packaged compiler, bindings,
  plugin, and WASM artifacts. Record commit, versions, results, known issues,
  and performance budgets. No unresolved wrong-code, data-loss, or critical
  security defects within the supported scope.
  Started: `scripts/qualify.ts` checks a distribution's checksums, installs
  it outside the checkout, and runs the whole suite through the installed
  launcher, and the package test with the packaged binary, writing a report
  of commit, host, runtimes and results; the [Qualify](.github/workflows/qualify.yml)
  workflow does it per host ([ADR 0094](docs/decisions/0094-qualification.md)).
  First qualified: the distribution of `4bd4366` for Linux x64, on GitHub's
  `ubuntu-latest`, with 605 tests through the installed launcher and the
  package test on the packaged binary, all passing; its four packages rebuilt
  byte for byte from the same source on another machine. Other hosts wait on
  correctness and completeness (M7–M10) before a release is made for them.
  The support matrix, the WASM compiler and performance budgets remain open.
- [ ] **M6.2 — Complete a pilot release and upgrade.** Deploy the pilot,
  observe it for an agreed period with agreed success criteria, fix blockers,
  and test an upgrade and rollback. Publish the support matrix, release notes,
  and remaining exclusions before calling the release production-ready.

## Toward Scala.js-level correctness and completeness: M7–M10

These tracks require sustained work beyond the first release. Their criteria
describe maturity to demonstrate, not a claim of equivalence today. Use Scala.js
as a reference for conformance discipline, library coverage, interop, and
tooling while preserving rust-js's own readable-output goals.

### M7 — Broad, composable Rust support

- [ ] **M7.1 — Maintain a language and std conformance inventory.** Enumerate
  Rust constructs and portable APIs, mark support and intentional semantic
  differences, and attach executable cases. Track feature combinations as well
  as isolated examples: generics with options, mutation through traits,
  nested patterns, iterators with effects, and async error paths.
  Started: the [corpus](test/corpus/) of `fn main()` programs, run natively
  and under Node, records support with `run-pass`/`run-fail`,
  rejections with `compile-fail`, and gaps with `ignore-rust-js`, which fails
  once a gap closes ([ADR 0088](docs/decisions/0088-corpus.md)). Its first
  cases found and fixed two miscompilations (nested element writes, repeated
  index effects in compound assignment). rustc's own `run-pass` UI tests run
  the same way (`bun run test:rustc`, [ADR 0089](docs/decisions/0089-rustc-tests.md)):
  1,425 of 2,691 in scope pass at the pinned toolchain, every other one is a
  clear rejection, none a crash or a wrong answer, and the
  [known failures](test/rustc-known-failures.txt) only shrink.
- [ ] **M7.2 — Close core representation gaps.** Design and implement the
  numeric, option, reference, slice, and resource-lifetime behavior needed for
  broad portable Rust. Include wider integers, `f32`, nested options, general
  supported mutation, and destructor/cleanup behavior. Establish explicit
  boundaries for raw memory and other native-only facilities. Readable output
  must not depend on accepting incorrect results.
  Progress: `i64`/`u64` are exact BigInts, with saturating float casts and
  integer `TryFrom` ([ADR 0086](docs/decisions/0086-64-bit-integers.md),
  [`wide` example](examples/wide.rs) compared with native Rust); 128-bit
  integers and `f32` remain.
- [ ] **M7.3 — Complete reusable abstraction support.** Extend associated
  types/constants, generic traits and methods, const generics, trait objects,
  closures, macros, and async composition against the inventory. Test them
  across modules and crates; remove application-specific special cases where
  general support is required.
- [ ] **M7.4 — Expand portable library coverage systematically.** Cover the
  agreed collection, text/Unicode, numeric, iterator, error, and serialization
  APIs. Verify operation sequences and edge cases against native Rust. Resolve
  the current gaps below and record any deliberate platform exclusions.
- [ ] **M7.5 — Run continuous conformance testing.** Maintain deterministic
  generated tests, fuzzing with minimized regression cases, and cross-engine
  execution. Track failures by compiler version and feature family. A feature
  is complete only when its contract and interaction cases pass.

### M8 — Reusable full-stack Rust crates

- [ ] **M8.1 — Build Cargo dependency graphs.** Support separately maintained
  shared crates, transitive dependencies, features, `cfg`, and the agreed
  build-script/procedural-macro model. Specify metadata and artifact versioning,
  module linking, dependency invalidation, and reproducible builds.
  Initial evidence: `tooling/cargo.js` discovers an offline, locked local-library
  graph with dependency aliases, resolved features and dependency ordering;
  `test/cargo.test.ts` exercises independent workspaces. `test/cargo-link.test.ts`
  now separately compiles and links a scalar path library, compares native/JS
  execution, and rejects stale inputs, incompatible identities and signatures
  before publication. See [ADR 0085](docs/decisions/0085-scalar-library-linkage.md).
  Automatic Cargo compilation, metadata/JS build-identity binding, transitive
  build coverage and cache invalidation remain open.
  First proof: a separate Cargo library with a non-generic scalar function,
  consumed through a path dependency by both a native executable and a rust-js
  application. Resolve the dependency from Cargo metadata, compile it separately,
  link its JS export through versioned dependency metadata, and compare results.
  A dependency edit must invalidate the consumer build; incompatible artifact
  identities must fail before publication. This narrow proof does not establish
  support for exported generics, trait evidence, build scripts or procedural macros.
- [ ] **M8.2 — Prove ecosystem compatibility.** Keep a versioned corpus of
  representative portable crates and real applications. Track each as builds
  unchanged, needs documented target adaptation, or blocked with a specific
  cause. Run the corpus on compiler and toolchain upgrades.
- [ ] **M8.3 — Share behavior as well as data.** Demonstrate the same model,
  validation, serialization, and domain-logic crates on a native Rust server
  and a rust-js client, including dependencies. Require integration and
  differential tests to agree across the client/server boundary.

### M9 — Mature JavaScript ecosystem and developer experience

- [ ] **M9.1 — Stabilize interop for reusable libraries.** Specify and test
  import/export conventions, callbacks, ownership at JS boundaries, nullish
  values, exceptions, promises, and public generic representations. Prove
  consumption from JavaScript and TypeScript with a usable typing strategy.
- [ ] **M9.2 — Make daily development dependable.** Provide editor diagnostics,
  navigation, completion, and formatting for supported Rust/JSX workflows.
  Maintain binding-generation and dependency-version tests. Exercise React
  composition, external npm libraries, debugging, and refresh in real projects.
- [ ] **M9.3 — Scale builds and output.** Measure large dependency graphs,
  incremental rebuilds, memory, dead-code removal through supported bundlers,
  helper duplication, and code splitting. Implement the improvements needed to
  meet published budgets while preserving semantics, readability, and maps.

### M10 — Demonstrate lasting production maturity

- [ ] **M10.1 — Establish independent production use.** Maintain multiple
  independently developed full-stack applications with different workloads.
  Record reliability, compiler blockers, upgrade cost, and performance over an
  agreed observation period. The compiler's own playground is one data point.
- [ ] **M10.2 — Sustain compatibility across releases.** Maintain a release
  policy, supported toolchain/dependency versions, regression triage, security
  response, and migration guides. Test old application sources and packages
  against new compiler releases according to that policy.
- [ ] **M10.3 — Reassess the end goal with evidence.** Review the conformance
  inventory, crate corpus, tooling coverage, unresolved defects, and production
  experience together. Keep missing ordinary application capabilities visible;
  do not declare completeness by narrowing the inventory after the fact.

## Confirmed feature gaps to prioritize

These are current boundaries checked against implementation or diagnostic
tests. M1.3 and the pilot identify first-release blockers; M7 and M8 track the
broader completeness work.

| Area | Current gap and evidence | Why it may matter |
| --- | --- | --- |
| Dependency reuse | General Cargo crate compilation remains outside the [documented scope](docs/decisions/0019-one-js-file-per-module.md). | Sharing a model file is easier than consuming an existing shared crate and its dependencies. |
| JSON models | Generic derives, `flatten`, every enum representation, and `Value` are supported ([ADRs 0079–0083](docs/decisions/0083-serde-json-value.md)); `with`, `serialize_with`, `deserialize_with`, and some `Value` methods remain rejected (see [Serde diagnostics](test/diagnostics.test.ts)). | Unusual API shapes may still need new support or a documented schema choice. |
| Numbers | [Numeric representations](src/lower/representation.rs) support 8/16/32-bit integers, `f64`, and `i64`/`u64` as BigInts ([ADR 0086](docs/decisions/0086-64-bit-integers.md)); `usize`/`isize` are 32-bit. rustc checks programs for `wasm32-unknown-unknown`, so constants, `size_of` and `cfg` agree with the 32-bit `usize` ([ADR 0090](docs/decisions/0090-wasm32-front-end.md)). 128-bit integers and `f32` are outside this set. | External schemas and numeric code may use them. |
| Traits and generics | [Validation](src/lower/traits.rs) rejects const generics, generic trait parameters, and generic trait methods; general associated items remain limited. | Existing Rust abstractions and dependencies may not compile unchanged. |
| Mutable references | [Primitive mutation boxes](docs/decisions/0074-mut-boxes.md) cover calls, with restrictions on returned/stored references and trait methods. | Reusable application helpers may exceed the current reference model. |
| Options, maps, and iterators | [Diagnostic cases](test/diagnostics.test.ts) include nullish concrete option payloads, struct map keys, map equality, and held-iterator restrictions. | Combinations matter even when each broad feature is listed as supported. |
| JSX authoring | [JSX boundaries](docs/jsx.md#current-boundaries) include macro composition and missing stock editor expansion. | Daily development and reusable component patterns need a tested workflow. |
| Text and slices | [Text contract](docs/decisions/0063-text.md) leaves UTF-8 byte offsets and mutable range slices unsupported; [diagnostics](test/diagnostics.test.ts) cover stored ranges. | Portable parsing and reusable algorithms depend on precise text and borrowing semantics. |
| Resource lifetime | [Trait contract](docs/decisions/0049-traits-and-generics.md) leaves user destructors outside the supported model. | Native RAII cleanup cannot be assumed to follow JavaScript garbage collection. |

## Public playground track

Track this separately from production readiness of generated applications.

- [ ] **P1 — Isolate executed programs.** The current
  [preview runner](wasm/web/rust/programs.rs) intentionally uses an unsandboxed
  frame. Define the trust model, isolate untrusted execution, and test parent
  access, message validation, runaway programs, and recovery before advertising
  it as safe for untrusted snippets.
- [ ] **P2 — Run React examples in the preview.** Today the playground displays
  generated JSX but its runner executes plain JS/DOM programs. Either add and
  test a JSX/React execution path or keep that boundary explicit in the UI.
- [ ] **P3 — Measure browser compiler reliability.** Establish download/startup
  and memory budgets, cancellation/recovery behavior, and cross-browser checks
  for the hosted compiler. Keep deployed compiler versions identifiable.

## Keeping this roadmap useful

Update the relevant checkbox in the same change that completes its acceptance
criteria. Add a commit/PR and test or measurement evidence beside completed
items. Use **in progress** only when linked work has started; use **blocked**
with a concrete dependency. Update the baseline when reassessing readiness.

New features belong under the gate they unblock. Record intentional exclusions
explicitly. Track completed gates and remaining blockers instead of assigning
an overall percentage to an undefined amount of work.
