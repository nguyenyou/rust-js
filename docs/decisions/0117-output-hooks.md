# 0117. A crate's settings for the JS rust-js writes: its formatter's options, and hooks

Status: Accepted, but for checks in Cargo's build, to come. Extends
[0065](0065-format-with-oxfmt.md) and [0114](0114-app-cargo-toml.md).

## Context

rust-js lays the JS out as oxfmt does, and moves the source map to the
formatted text by pairing the nodes of the two (ADR 0065). It lays it out
at oxfmt's defaults, 100 columns and the rest, which a team may not have
chosen. And a team has its own tools for JS: a formatter with its own
settings, Prettier's, Biome's or dprint's; a linter, ESLint or oxlint; a
type checker, `tsc` with `checkJs`. Each runs on what's on disk, and none
can be told the JS came from Rust: a lint of `App.jsx` says a line of it,
which nobody edits, and a formatter run after rust-js leaves the map
pointing at where things were.

The pairing needs nothing of oxc's formatter but its output: it parses the
text before and after, and holds for any tool that moves code without
changing what it is.

## Decision

**A crate's `Cargo.toml` can set the formatter's options, and name commands
rust-js runs on the JS it writes**, of two kinds:

```toml
[package.metadata.rust-js.format]
printWidth = 120

[package.metadata.rust-js.hooks]
transform = [
  ["prettier", "--stdin-filepath", "{file}"],
]
check = [
  { run = ["oxlint", "-f", "unix", "{files}"], fail = "never" },
  { run = ["tsc", "--noEmit", "--pretty", "false", "-p", "."], when = "build" },
]
```

With neither, rust-js writes what it wrote before: oxfmt's layout.

### The formatter's options are oxfmt's

- **By oxfmt's names, Prettier's**, which rust-js reads as oxfmt does, so
  its documentation is theirs: `printWidth`, `tabWidth`, `useTabs`,
  `endOfLine`, `singleQuote`, `jsxSingleQuote`, `quoteProps`,
  `trailingComma`, `semi`, `arrowParens`, `bracketSpacing`,
  `bracketSameLine`, `singleAttributePerLine` and `objectWrap`.
- **Over rust-js's own**: oxfmt's defaults, but `objectWrap = "collapse"`
  (ADR 0065), which a crate can set back to `"preserve"`.
- **One oxfmt hasn't, or a value it can't take, fails the build**, with
  the file and why: a misspelled `printWidht` isn't a layout quietly lost.

### Transforms change a module's text

```
 printed JS ─► oxc format ─► transform 1 ─► transform 2 ─► written, with its map
                   │               │              │
                   └── paired ─────┴── paired ────┘   each step's map moved
```

- **Each is run for each module, before it's written**: the module's text
  on its stdin, the new text on its stdout, `{file}` the path it will have,
  in the crate's directory. They run in order, the first on oxc's
  formatted text, which keeps a layout a formatter would keep, as Prettier
  keeps an object on the lines it's on (ADR 0065).
- **Its text must be the same program, laid out otherwise**: the same
  nodes, in the same order, but for JSX's text and the `{" "}` that stands
  for its spaces, which a formatter breaks otherwise; parentheses aren't
  nodes. So `let` to `const` is a transform, and a fix that takes a
  statement out isn't.
- **A transform's text that isn't is refused, and its input kept**: one that
  exits otherwise than 0, can't be run, or changes the program, is warned
  of, with why, `took out a ReturnStatement`, and the module is written as
  it was before that step. So no hook makes the map point at the wrong code.

### Checks read what was written

- **Each is run once a build has written its files**, as a type checker
  needs the whole program, not a module: `{files}` is every file the build
  wrote, one argument each, in the crate's directory. It changes nothing.
  - **In a crate compiled alone**, rust-js runs them when it's done.
  - **In Cargo's build** (ADR 0101), which runs rust-js for each crate, its
    driver is to run them, once Cargo is done. Until it does, it runs none.
- **What it says of the JS is said of the Rust**: a line of its output that
  names a file the build wrote, at a line and column, is moved through
  that file's source map to the Rust it came from, and named for the tool:

  ```
  src/App.jsx:14:7: 'x' is never reassigned. Use 'const' instead.
     ──►  src/App.rs:22:5: [oxlint] 'x' is never reassigned. Use 'const' instead.
  ```

  rust-js reads the two ways tools write a place, `file:line:col: ..`,
  ESLint's and oxlint's `-f unix`, and `file(line,col): ..`, `tsc`'s with
  `--pretty false`. A line in neither, or at JS no Rust wrote, a runtime
  helper's, is passed on as it is.
- **`fail` says what its failure is**, an exit otherwise than 0: `"build"`,
  the default, fails the build, and in the Vite server shows in its
  overlay, as a compile error does; `"never"` reports what it said as
  warnings, Vite's too.
- **`when` says which builds run it**: `"always"`, the default, each one,
  and each save in the Vite server; or `"build"`, `vite build` and the CLI,
  not the server, for a check too slow for a save. rust-js is told which by
  `--hooks save`, which the Vite server gives it, or `--hooks build`, the
  default.

### Where they're read

- **The crate's `Cargo.toml`**, which every app has (ADR 0114): Cargo's
  for a crate Cargo builds, `CARGO_MANIFEST_DIR`, or, for a crate compiled
  alone, the nearest one above its root file.
- **Not in the playground**, which runs no process and has no `Cargo.toml`,
  nor in rust-js's own tests and snapshots (ADR 0050): what they check is
  rust-js's output, not a tool's.

## Why

- **One hook for every tool is two, not one**: a transform changes the
  text, so the map must follow it; a check reads it, so what it says must
  be moved to the Rust. A tool is one or the other.
- **Pairing makes a transform safe to allow**: it's how oxfmt's layout
  keeps the map right, and it tells a layout from another program, so a
  hook can't break debugging, only be refused.
- **A check is only worth running if it speaks of the Rust**: that's what's
  edited. Moving a place through the map is what a debugger does already.
- **The options are oxfmt's names** because the formatter is oxfmt's: a
  team that has an `.oxfmtrc` or a Prettier config writes the same keys.
- **The crate's `Cargo.toml`** is read by every way rust-js builds it, the
  CLI, Cargo's and the Vite plugin's, where `package.json` is only Vite's.

## Alternatives

- **No transform, only oxc's formatter**, and a team's formatter run after
  rust-js: its map would be wrong, and a debugger would stop at other code.
- **Hooks as JS functions in the Vite plugin**, `format(code, file)`: no
  process to start, but only for a Vite build, and the pairing would have
  to be done again in JS, or exposed from rust-js.
- **Allowing a transform that changes the program, its map made
  approximate**: fixes like removing an unused variable would work, and
  every debugger session would be in doubt.
- **Reading an `.oxfmtrc.json`**: the options would be where oxfmt's are,
  but in a file of their own, and rust-js would read two.

## Consequences

- **The tests**: a transform's text written, and the map moved to it; one
  that changes the program, or fails, refused, and the next given its
  input; Prettier's layout of the vite-react app's JSX kept, at its own
  80 columns, and each probe of the map finding the Rust it did before;
  what a check says, of both forms, said of the Rust, and a line of
  neither as it is; `fail`; `when`, for the CLI and in Vite; and the
  formatter's options, `printWidth`, `tabWidth`, `singleQuote` and `semi`,
  the map following them, and ones oxfmt hasn't refused.
- **A mapping inside a node still keeps its offset from the node's start**
  (ADR 0065): a formatter that moves a node's child to a line of its own,
  as Prettier does `{count}`, leaves the child on a line with no mapping.
- **A transform starts a process for each module**, which a Node tool's
  start, a few hundred milliseconds, makes slow; a native one, Biome's or
  dprint's, much less.
- **The committed JS is the team's tools' too** (ADR 0041): what it is
  depends on their versions and settings, as any formatted code's does.
- **rust-js reads `Cargo.toml` with the `toml` crate**, natively only; the
  formatter's option types are `oxc_formatter_core`'s, of the same tag.
- **Still to come**: checks in Cargo's build, by its driver, once Cargo is
  done.
