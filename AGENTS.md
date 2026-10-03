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
amendments, and the existing implementation and tests. The
[architecture](docs/architecture.md) says where a change goes, and which
boundaries [its test](test/architecture.test.ts) holds it to. Verify uncertain
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

## Develop in a Linux VM

On macOS, the system scans each newly built binary before its first run, one
at a time, so tests that build native programs, the corpus, rustc's tests,
generated programs and mutations, take many times longer there than on
Linux: rustc's `drop` tests took 23 seconds against 1. So develop in a
[Tart](https://tart.run) Linux VM on the same Mac, and run every check
there, through [`scripts/linux-vm.sh`](scripts/linux-vm.sh):

```bash
scripts/linux-vm.sh 'bun run test'
```

It starts the VM if it isn't running, syncs this checkout into the VM's own
copy, uncommitted edits too, and runs the command there. What git ignores,
`target/`, `node_modules/` and the WASM build's, is the VM's own. What the
command writes that git doesn't ignore, a blessed snapshot or list, is
copied back to this checkout, so a bless there is a bless here. Edit
here, and run there:

| To | Run in the VM |
|---|---|
| Test | `bun run test`, or `bun test test/<file>.test.ts -t <name>` |
| Bless snapshots | `bun run bless` |
| Put a known bug back, and see its tests catch it ([ADR 0093](docs/decisions/0093-mutations.md)) | `bun scripts/mutations.ts <name>`, or all of them with no name |
| Check rustc's tests against the known failures ([ADR 0089](docs/decisions/0089-rustc-tests.md)) | `bun run test:rustc`, or some: `bun run test:rustc drop/ closures/` |
| Bless rustc's tests' lists | `bun run test:rustc:bless` |
| Run generated programs ([ADR 0092](docs/decisions/0092-generated-programs.md)) | `cargo build && FUZZ_START=1000 FUZZ_SEEDS=600 bun test test/fuzz.test.ts`; the reduced programs stay in the VM's `target/fuzz/` |
| Build the playground's compiler | `bun run wasm` |

Before pushing a change, run what the [check workflow](.github/workflows/check.yml)
runs, `bun run typecheck`, `bun run fmt:check`,
`cargo clippy --locked -- -D warnings`, `cargo test --locked`,
`bun run test` and `bun run --cwd examples/vite-react build`; after
`bun run wasm`, `RUST_JS_REQUIRE_WASM=1 bun test test/snapshots.test.ts test/playground.test.ts`;
and rustc's tests, all of them, in about a minute. Bless their lists there
when a change moves them, and commit the lists with the change. The VM
gives the same verdicts as the workflow's x86 machines: a test either one
ignores is out of scope.

The [workflows](.github/workflows) run the same checks on GitHub's
machines, when started by hand. Run them now and then, as before a
release, to confirm an x86 machine agrees: `gh workflow run check.yml`, and
`gh workflow run "rustc tests"` with `-f bless=true`, `-f mutations=true`,
or `-f fuzz_start=1000 -f fuzz_seeds=600`.

To set the VM up once, on Apple Silicon:

```bash
brew install openai/tools/tart
tart clone ghcr.io/cirruslabs/ubuntu:latest rustjs
tart set rustjs --cpu 10 --memory 24576 --disk-size 200
scripts/linux-vm.sh true   # boots it, with this checkout shared
```

Then, in the VM (`tart exec rustjs bash -l`), install what the workflows
use: `build-essential`, `pkg-config` and `rsync` from apt; rustup, with the
toolchain and components [rust-toolchain.toml](rust-toolchain.toml) pins,
and the `wasm32-wasip1` and `wasm32-unknown-unknown` targets; the Bun
version the workflows set up, in `~/.bun`; Node 24; and, in the VM's copy,
`bun install` and `bunx --bun playwright install --with-deps chromium`.
For `bun run wasm`, check out rustc's source in the VM's `wasm/rustc` as
the check workflow's `wasm-parity` job does. Mount the shared checkout at
boot with an `/etc/fstab` line for `/mnt/shared` (`virtiofs`). Two things
a managed network may need, or downloads fail or hang: a TLS-inspecting
proxy's root certificate in the VM's `/usr/local/share/ca-certificates`,
and a smaller MTU, such as 1280 in a netplan file, where a tunnel drops
full-size packets.
