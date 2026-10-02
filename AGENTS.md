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
[package.json](package.json) for commands: `bun run build`, `bun run test`, and
`bun run fmt:check`. Rebuild with `bun run wasm` when validating compiler
changes through the browser playground.

## Tests that build many native programs

On macOS, the system scans each newly built binary before its first run, one
at a time. A run that builds hundreds of native test programs therefore takes
an hour or more locally, though on Linux it takes minutes. For those runs,
use the manually started [rustc tests workflow](.github/workflows/rustc-tests.yml)
instead of running them locally:

- rustc's own tests, all of them ([ADR 0089](docs/decisions/0089-rustc-tests.md)):
  `gh workflow run "rustc tests" --repo rust-js-lang/rust-js`, with
  `-f bless=true` to rewrite the known failures, the tests native Rust
  gives no answer for, those out of scope, and the inventory of all of them.
  The rewritten lists are the run's
  `rustc-known-failures` artifact; download it with `gh run download` and
  review their diffs before committing them.
- Some of rustc's tests, by file or directory under `tests/ui`:
  `-f tests="derives/ consts/std/iter.rs"`.
- A batch of generated programs ([ADR 0092](docs/decisions/0092-generated-programs.md)):
  `-f fuzz_start=1000 -f fuzz_seeds=600`. A program that differs is reduced,
  and the reduced programs are the run's `fuzz` artifact.
- Known bugs put back into the compiler, each of which its tests must
  catch ([ADR 0093](docs/decisions/0093-mutations.md)): `-f mutations=true`,
  or `bun scripts/mutations.ts copy-on-read` for one or two locally.

A handful of tests, such as the corpus in `bun run test` or a few named rustc
tests, is fine locally: the suite keeps each native program it builds in
`target/native-cache/`, so a second run builds and scans none of them
([ADR 0104](docs/decisions/0104-parallel-tests.md)). Start the workflow, share the run's link, and read
its results when it has finished.

### A Linux VM on macOS

A Linux VM on the same Mac doesn't scan its binaries. Run in one, the same
rustc tests took about a twenty-fifth of the time they took on macOS (the
`drop` directory: 1 second against 23), with the same verdicts. So where a
[Tart](https://tart.run) VM is set up, run tests that build native programs
there, through [`scripts/linux-vm.sh`](scripts/linux-vm.sh):

```bash
scripts/linux-vm.sh 'cargo build --release && bun scripts/rustc-suite.ts drop/ closures/'
```

It starts the VM if it isn't running, syncs this checkout into the VM's own
copy, uncommitted edits too, and runs the command there. The VM's `target/`
and `node_modules/` are its own. Use it for named rustc tests or
directories, the corpus, and a few mutations.

All of rustc's tests take about a minute there, so check a change against
the known failures in the VM before pushing it:

```bash
scripts/linux-vm.sh 'cargo build --release && bun scripts/rustc-suite.ts'
```

On an Arm VM, expect one difference, and ignore it:
`abi/abi-sysv64-arg-passing.rs`, which is x86-only (`ignore-aarch64`), is
reported "newly with a native answer", since the workflow's x86 machines
can't link it natively. Anything else the check reports is the change's.
The workflow above stays the record of rustc's tests and their known
failures: bless there, and commit the lists it writes.

To set the VM up once, on Apple Silicon:

```bash
brew install openai/tools/tart
tart clone ghcr.io/cirruslabs/ubuntu:latest rustjs
tart set rustjs --cpu 10 --memory 24576 --disk-size 200
scripts/linux-vm.sh true   # boots it, with this checkout shared
```

Then, in the VM (`tart exec rustjs bash -l`), install what the workflows
use: `build-essential`, `pkg-config` and `rsync` from apt; rustup, with the
toolchain and components [rust-toolchain.toml](rust-toolchain.toml) pins;
the Bun version the workflows set up, in `~/.bun`; and Node 24. Mount the
shared checkout at boot with an `/etc/fstab` line for `/mnt/shared`
(`virtiofs`). Two things a managed network may need, or downloads fail or
hang: a TLS-inspecting proxy's root certificate in the VM's
`/usr/local/share/ca-certificates`, and a smaller MTU, such as 1280 in a
netplan file, where a tunnel drops full-size packets.
