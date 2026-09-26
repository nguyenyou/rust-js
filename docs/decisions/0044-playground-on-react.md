# 0044. The playground on React, a slice at a time

Status: Accepted, in progress. Extends [0032](0032-dogfooding-the-playground.md).

## Context

The playground is Rust compiled by rust-js (ADR 0032), but written the way
the DOM was written before frameworks. It has 20 `thread_local!` globals
(the files, the current file, the outputs, the compiler), and functions that
rebuild parts of the page by id, like `render_tree`. The `react` crate now
binds React's whole API (ADR 0043). Writing the playground with it tests
that binding the way ADR 0032 tested the compiler: a real app, whose every
gap is one a user would hit.

Two things make the playground different from the Vite example:

- **The compiler is the WebAssembly one.** `compile-rust.ts` builds the page
  with `rust-js.wasm`, under the WASI shim, as the page itself compiles.
- **Bun, not Vite, bundles the page,** from `index.html`.

## Decision

**Port it in slices, keeping the site working after each**, as ADR 0032 did:

1. **Plumbing, and a React root hosting the page (done).**
   - `index.html`'s body is a `#app` element.
   - `rust/page.rs` renders the page's structure as an `App` component.
     `start()` renders it synchronously, with `flush_sync`, so the rest of
     `lib.rs` finds its parts by id as before.
   - The editors are made as the module loads, before React renders, so
     each is made on an element of its own, and `start()` moves it in. The
     Result frame is found on first use.
   - The page gets the `react` crate like any project: `react` and
     `react-dom` in `wasm/web/package.json`, and the crate built for
     `wasm32-unknown-unknown` for that React version (`buildReactCrate`).
     `compile-rust.ts` hands it to the WASI compiler and collects `.jsx` output.
2. The toolbar, status line and example picker, as components with state.
3. The file trees, as a recursive component.
4. The editors, as a component that makes its `EditorView` in an effect and
   destroys it in the cleanup.
5. The Result frame and the test reports; the `thread_local!`s go.

**A smoke test in Chromium** (`test/playground.test.ts`) builds the static
site and serves it as GitHub Pages does. It loads the page, runs the first
example's tests, and compiles the modules example, with no page errors. The
playground had no automated UI test before; the port needs one.

## Why

- **It's the React binding's hardest user:** a CodeMirror integration,
  async work in handlers, a WebAssembly compiler, and an iframe it messages
  with. The pure-React tests don't reach that.
- **Slices keep it shippable.** Slice 1 changes no behavior, so the smoke
  test pins the old behavior before any of it moves into components.

## Consequences

- The page loads React and React DOM: 211 KB minified, 66 KB gzipped (measured with `bun build --minify`).
- Until slice 5, React renders the structure while the old code owns what's
  inside it. React has no state yet, so it never re-renders over that.
- StrictMode waits for slice 4. It runs effects twice in development, which
  would make the old code's editors twice.
