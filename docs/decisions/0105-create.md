# 0105. `bun create @rust-js` makes a Vite and React app in Rust

Status: Accepted. Builds on [0041](0041-react.md), [0094](0094-qualification.md)
and [0103](0103-runtime-package.md).

## Context

The vite-react example is the app a new user wants: create-vite's React
template, its component written in Rust, React Compiler and Tailwind CSS.
Getting it means cloning this repository and building it. `bun create vite`
is how a Vite user starts, and `bun create <name>` runs the package named
`create-<name>`; `bun create @scope`, as `npm init @scope` does, runs
`@scope/create`.

## Decision

**`@rust-js/create` copies the vite-react example into a new directory**:

```bash
bun create @rust-js my-app      # with Bun
npm create @rust-js my-app      # with Node.js
```

- **Its template is the example's files, as git has them**, packed beside
  its script as `template/` (`bun run pack:create`): nothing building the
  example made goes with it, and there's one app to keep up, not two. A file
  in `create/app/` takes the place of the example's own: its README
  is the app's, not this repository's. npm leaves a package's `.gitignore`
  out, so it's packed as `_gitignore`, and named back.
- **The app is named for its directory, and rust-js's packages are this
  release's version**, where the example has the workspace's. It's released
  with the compiler, at its version, and knows its Rust pin, which it says
  to install once: `rustup toolchain install <pin> --profile minimal
  --target wasm32-unknown-unknown`.
- **Bun or Node.js, either.** The app is theirs alike, and what to run next
  is the package manager's that ran it, as it says in `npm_config_user_agent`
  (npm, Bun, pnpm and Yarn each set it), or else the runtime's.
- **What to install first, and how, is its README's**: Bun or Node.js, rustup, and that
  Rust, about 500 MB. Not `rustc-dev`, which building rust-js needs and
  running it doesn't: it runs with rustc's own library, in the minimal
  profile's `rustc`. Node.js must be Vite 8's even beside Bun: Bun runs
  Vite on it when it's installed, and stands in for it when it isn't.
- **`@rust-js/resources` asks for only that Rust.** rustup installs what a
  directory's `rust-toolchain.toml` lists before it runs `rustc` there, as
  `react/build.sh` does, and this repository's lists `rustc-dev` and its
  tools: the package's has the pin and the target alone.
- **With `--local <distribution>`**, the app installs the packages
  `pack:distribution` made, as Qualify does: each the app's own, and
  `@rust-js/build` the plugin's too. A distribution of another host or
  version is refused. It's how the template runs before a release is on npm.
- **It never writes into a directory that isn't empty.**
- **It's `@rust-js/create`**, in the scope every package of this repository
  is named in (ADR 0103): `bun create @rust-js` runs it, as bunx's
  `add_create_prefix` makes `@org` `@org/create`. Its one command, a string
  `bin`, is its unscoped name, `create`.

## Why

- **The app is the example.** Its tests, its compile and its build are
  already the repository's; a template of its own would drift from them.
- **A distribution is what a release will publish.** An app made with
  `--local` runs what npm will serve, from the same archives.

## Alternatives

- **A GitHub template repository, or `degit` of the example:** no package
  to publish, but the app would name the workspace's packages, and nothing
  would say which Rust to install.
- **`create-rust-js`**, `bun create rust-js`: the command a Vite user knows,
  but the one name outside the scope.

## Consequences

- Tried outside the repository: `@rust-js/create --local` of this host's
  distribution made the app, `bun install` installed it, and `bun run
  build` compiled `App.rs`, the React bindings for the React it installed
  first, with Vite 8. An edit to `App.rs` was in the next build.
- Tried with a rustup of its own, so nothing installed before counted: the
  minimal profile and the target were four components, 512 MB, in 50
  seconds, and the app built with them, the React bindings too, and with no
  Node.js on the `PATH`. That rustup, with this repository's
  `rust-toolchain.toml`, would have installed `rustc-dev`, `llvm-tools`,
  `rust-src`, `rustfmt` and `clippy` for the app's first build (rustup's
  `ensure_installed`).
- Tried with npm on Node.js too: `npm exec` of the packed tarball made the
  app and said npm's commands, and `npm install` and `npm run build`
  compiled `App.rs` and built it.
- `bunx` and `npm exec` take a local tarball by an absolute `file:` path.
- `bun create @rust-js` itself needs `@rust-js/create` and the packages on npm,
  which nothing publishes yet. The app's `package.json` names `@rust-js/runtime`
  and `@rust-js/vite-plugin`; `@rust-js/build` is to bring the compiler and
  its resources, one package for each host.
