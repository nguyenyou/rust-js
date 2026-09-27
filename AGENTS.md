# rust-js: north star

**Rust in. Readable JavaScript out.**

Build a compiler that lets people write Rust and get JavaScript they would
have been willing to write by hand. The longer-term goal is Rust across the
application stack: native Rust on the server, rust-js on the frontend, and
shared types and data models between them.

Aim for Scala.js-level correctness and completeness for full-stack Rust:
broad language and library support, reusable shared crates, and dependable
interop and tooling. The first production app is an intermediate milestone.

## Principles

- **Keep Rust's checks.** rustc owns types, traits, ownership, borrow checking,
  and diagnostics. Never bypass its checks to make a feature compile.
- **Treat generated code as a product.** Names, control flow, modules, JSX,
  formatting, and source maps must be understandable to JavaScript developers.
  Inspect the output when changing the compiler.
- **Use JavaScript's building blocks.** Prefer objects, arrays, functions,
  promises, and ES modules. Emit helpers only where the supported behavior
  needs them. Keep compiler code and generated code simple.
- **Make semantics explicit.** Preserve the established contract, including
  evaluation order, side effects, copying, overflow, and errors. Differences
  from native Rust must be deliberate, documented, and tested. Readability
  does not justify accidental behavior changes.
- **Reject unsupported features clearly.** Report useful compiler errors;
  never silently approximate behavior or publish partial output on failure.
- **Make interop fundamental.** Browser APIs, React, npm, Vite, and JavaScript
  callers are core concerns. Preserve public representations and interfaces.
  Keep the native and browser compilers consistent.

## Making changes

Read the relevant [design decisions](docs/README.md), including later
amendments, and the existing implementation and tests. Verify uncertain
library behavior from local source. Follow the existing compiler phases and
reuse evaluation-order machinery.

Start with a small Rust example and the JavaScript it should produce. Compare
behavior with native Rust where they should agree; test intentional differences
explicitly. Review generated-code snapshot diffs before accepting them. Use
real examples and the playground to check integration when relevant.

Document new semantic choices and current limitations. Run checks appropriate
to the change and report what was verified. Documentation-only edits need
content and link checks.

Track production-readiness work in [ROADMAP.md](ROADMAP.md). Update relevant
items with evidence when their acceptance criteria are met.

Use the pinned Rust toolchain and Bun for JavaScript tooling. See
[package.json](package.json) for commands: `bun run build`, `bun test`, and
`bun run fmt:check`. Rebuild with `bun run wasm` when validating compiler
changes through the browser playground.
