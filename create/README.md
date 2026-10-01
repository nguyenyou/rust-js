# @rust-js/create

```bash
bun create @rust-js@latest my-app      # with Bun
npm create @rust-js@latest my-app      # with Node.js
```

makes `my-app`, a Vite and React app whose component is written in Rust
([ADR 0105](../docs/decisions/0105-create.md)). It's the
[vite-react example](../examples/vite-react)'s files, named for their
directory, with rust-js's packages at this release's version, and it says
the Rust release rust-js runs with, to install once. Either runs
`@rust-js/create`, and it says the next commands of the one that ran it:
`bun install` and `bun run dev`, or `npm install` and `npm run dev`.

## Before you start

Three things, each installed once:

| What | Why | How |
| --- | --- | --- |
| [Bun](https://bun.sh) or [Node.js](https://nodejs.org) | makes the app, installs it, and runs Vite: either one | Bun: `curl -fsSL https://bun.sh/install \| bash`. Node.js, `^20.19.0` or `>=22.12.0`, and its npm: nodejs.org's installer, or a version manager's |
| [rustup](https://rustup.rs) | installs and chooses Rust toolchains | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| rust-js's Rust | the stable release rust-js is built with: rust-js runs with its rustc's own library, and checks the app for its `wasm32-unknown-unknown` target | `rustup toolchain install 1.98.1 --profile minimal --target wasm32-unknown-unknown` |

- **The Rust is about 500 MB**: the minimal profile, rustc, Cargo and the
  standard library, for this machine and for `wasm32-unknown-unknown`. Not
  `rustc-dev`: that's for building rust-js, not for running it. rustup keeps
  it apart from the Rust you use, and making the app prints the command too.
- **Bun or Node.js, either.** Node.js must be one Vite 8 runs on, `^20.19.0`
  or `>=22.12.0`, with Bun too if it's installed: `bun run dev` runs Vite on
  Node when there is one, and on Bun when there isn't.
- **macOS or Linux**, on a machine `@rust-js/native`, the compiler, is built
  for.

## Before a release is on npm

Give it a distribution, the directory
`bun run pack:distribution` makes, and the app installs its packages.
`bunx` and `npm exec` take the packed tarball by an absolute `file:` path,
and run its command, `create`:

```bash
bun run pack:create target/create.tgz
bunx --package "file:$PWD/target/create.tgz" create my-app --local <distribution>
npm exec --package "file:$PWD/target/create.tgz" -- create my-app --local <distribution>
```

It never writes into a directory that isn't empty.
