# 0044. The playground on React, one component per file

Status: Accepted. Extends [0032](0032-dogfooding-the-playground.md).

## Context

The playground is Rust compiled by rust-js (ADR 0032), but it was written the
way the DOM was written before frameworks. It had 20 `thread_local!` globals
(the files, the current file, the outputs, the compiler), and functions that
rebuilt parts of the page by id, like `render_tree`. The `react` crate binds
React's whole API (ADR 0043). Writing the playground with it tests that
binding the way ADR 0032 tested the compiler: a real app, whose every gap is
one a user would hit.

It's also the example of a large app broken into components. The Vite
example (ADR 0041) is one component.

Two things make the playground different from the Vite example:

- **The compiler is the WebAssembly one.** `compile-rust.ts` builds the page
  with `rust-js.wasm`, under the WASI shim, as the page itself compiles.
- **Bun, not Vite, bundled the page,** from `index.html`. It became a Vite
  app in [0045](0045-playground-on-vite.md).

## Decision

**Components, one per file, in `rust/components/`.** Each file is a JS
module (ADR 0019) that exports only components, so saving one is a Fast
Refresh of that component alone:

```text
App                      the state, and the page's layout
├── Toolbar              the example, Compile and Test, the status
│   ├── ExamplePicker
│   └── StatusLine
├── Pane "Rust"          a heading, a file explorer beside an editor
│   ├── FileTree         a folder's entries; a folder's own are a FileTree inside
│   │   └── FileItem     a file: open it, or delete it
│   └── Editor           CodeMirror, made in an effect
├── Pane "JavaScript"
│   ├── FileTree
│   └── Editor
├── ResultFrame          runs the program, and hears how it went
└── StatsTable
```

The page's entire component tree, including its `StrictMode` root, uses
`jsx!` markup ([0072](0072-jsx-syntax.md)). Hooks and callback wiring remain
ordinary Rust; tags, props, fragments and keyed lists use the shared syntax
that the native and WASM compilers both expand.

What isn't a component is in modules of its own beside `components/`:
`compiler.rs` (downloading and running `rust-js.wasm`), `programs.rs`
(linking the output, and the Result frame's page), `codemirror.rs`,
`projects.rs` (the crate being edited), `tree.rs`, `styles.rs` (the Tailwind
classes several components share) and `dark_mode.rs`, a hook.

**State lives in `App`, in hooks, and goes down as props. What happens goes
up as callbacks** (`on_compile`, `on_open`, `on_outcome`) that set it:

- **One `use_state` for each concern**: what's loaded, the status, the
  project, the output, the program to run, the stats. There's no reducer: a
  reducer's state would be one struct, and Rust's `..*state` can't copy its
  `Vec`s out of a reference.
- **State is replaced, never changed.** `projects.rs` makes a new
  `Project` for each edit: `project.opening(path, live)` is the project
  with another file open.
- **Compile is a Transition** (`use_transition`), so `compiling` is React's
  own pending flag. It disables the buttons until the async compile is
  done.
- **Each file keeps its CodeMirror state**, so its undo history survives
  switching. `App` holds the source editor's view in a ref, and reads the
  open file's latest edits from it when it switches files or compiles.
- **`Editor` makes its view in an effect, and destroys it in the cleanup.**
  A second effect shows a new state, and follows the theme. ⌘/Ctrl-Enter is
  a React `onKeyDownCapture` on its element. That runs before
  CodeMirror's own Mod-Enter, and always calls the latest `on_compile`.
- **`ResultFrame` keys its `<iframe>` by the run's number**, so each run
  gets a new frame. It listens for the frame's report in an effect, and an
  `AbortController` in the cleanup removes the listener and the timeout's
  report.
- **`use_dark_mode` is a hook**, over `use_sync_external_store` and
  `matchMedia`. It's `useDarkMode` in JS, the name React finds a hook by,
  since the crate is `#![rust_js::camel_case]` (ADR 0046). So are the
  props: `on_open` is `onOpen`.
- **`StrictMode`** is on. In development it runs each effect twice, so the
  loading effect drops its first run's results.

**A smoke test in Chromium** (`test/playground.test.ts`) builds the static
site and serves it as GitHub Pages does. It:

- loads the page and runs the first example's tests;
- compiles the modules example and switches output files;
- compiles an unsaved edit with ⌘/Ctrl-Enter, and checks that rustc's
  errors show;
- checks for no page errors throughout.

## Why

- **It's the React binding's hardest user:** a CodeMirror integration,
  async work in a Transition, a WebAssembly compiler, and an iframe it
  messages with. The pure-React tests don't reach that.
- **Components in files of their own are how a React app is organized,**
  and it's what Fast Refresh needs: a module whose exports are components.

## Consequences

- The page loads React and React DOM: 211 KB minified, 66 KB gzipped (measured with `bun build --minify`).
- A module that a component imports is imported as a namespace
  (`import * as projects`), so a module named like a variable would rename
  it: `project$1`. That's why the modules have plural names, `projects` and
  `programs`.
- What the port found missing in rust-js was either filled or worked
  around:
  - Filled: `match` on string literals, `Option::map`, and `with` closures
    put in place.
  - Filled next: names, with `#![rust_js::camel_case]` (ADR 0046),
    methods (ADR 0047), so `projects.rs` is `impl Project`, and let
    chains (ADR 0048).
