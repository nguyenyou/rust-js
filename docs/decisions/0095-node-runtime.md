# 0095. The JS rust-js makes runs on Node; Bun runs the tests

Status: Accepted. Amends [0084](0084-owned-phases-and-host-boundaries.md), [0088](0088-corpus.md), [0089](0089-rustc-tests.md) and [0092](0092-generated-programs.md).

## Context

Every program the corpus, rustc's tests and the generated programs check
ran as JS under Bun and under Node, and the distributed tooling was tested
under both. Bun is still becoming compatible with Node, and a difference
of its showed as a wrong answer of rust-js's: a program that prints with
`print!` and `println!`, which are `process.stdout.write` and
`console.log`, lost what `console.log` wrote, 8 runs of 200 under Bun on
Linux, through a pipe, and none of 200 under Node.

## Decision

**The JS rust-js makes, and the tooling it ships, target Node.** The
corpus, rustc's tests and the generated programs run each program's JS
under Node, as it's compiled and, in the corpus, as Vite ships it; the
package test runs the installed tooling under Node, with Bun out of reach
on its `PATH`, so it can't come to need it.

**Bun is the tooling that runs the tests,** as it is the checkout's
package manager and script runner: `bun test`, `bun scripts/..`, and the
generated tests of `rust-js --test`, which ADR 0026 runs with `bun test`.

## Why

- **One runtime is one answer to compare with native Rust's:** a
  difference is rust-js's to fix, not a runtime's still on its way.
- **Node is where the JS is run,** by a server, a build, or the browser
  tooling a Vite app has.

## Consequences

- A difference of Bun's, as the one above, isn't worked around: a
  program's output under Bun isn't what's promised.
- `rust-js --test` still makes tests for `bun test` (ADR 0026); whether
  they should run under Node's own runner is a question for later.
