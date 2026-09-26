# 0045. The playground is a Vite app, with React Compiler and Tailwind

Status: Accepted. Extends [0044](0044-playground-on-react.md), whose context
said Bun bundled the page.

## Context

Once React rendered it (ADR 0044), the playground was a React app bundled by Bun:
`serve.ts` for development and `build.ts` for the static site. Each one
compiled `rust/lib.rs` with `rust-js.wasm` first, then bundled `main.ts`.
The Vite example (ADR 0041) is how the docs say to use rust-js with React. It
has Vite, `@vitejs/plugin-react`, Fast Refresh, React Compiler and Tailwind.
The playground, the app we use most, had none of those.

What makes the playground different is its compiler. It is compiled by the
WebAssembly build, under the WASI shim, as the page compiles its users'
code. `vite-plugin-rust-js` runs the native `rust-js` binary.

## Decision

**`wasm/web` is a Vite 8 app, set up like `examples/vite-react`:**

```text
vite.config.ts
  playgroundFiles()   site.ts: rust-js.wasm, the sysroot, the web crate and
                      the examples, served in dev and emitted by the build
  rustJs({ compile }) vite-plugin-rust-js, compiling with compile-rust.ts
  react()             plugin-react 6: JSX, Fast Refresh
  babel(reactCompilerPreset())
  tailwindcss()       scans rust/*.rs for class names
```

- **The plugin gets a `compile` option:** `({ crate, output, manifest }) =>
  Promise<void>`. With it, the plugin doesn't build crates or run the binary.
  It calls the function, then reads the manifest (ADR 0042) as usual. So
  what's watched, what's reloaded and how errors show all work as with the
  binary. The playground's `compileRust` runs `rust-js.wasm` with
  `--manifest` and translates the manifest's paths out of the WASI
  filesystem. It writes only the outputs that changed, so Vite updates only
  those.
- **`bun run dev`** is `vite` on port 4400, and **`bun run site`** is
  `vite build` to `wasm/web/dist`. `bun run preview` still serves `dist`
  under `/rust-js/`, as Pages does. `serve.ts` and `build.ts` are gone.
- **Vite runs on Bun, and loads its config natively**
  (`bunx --bun vite --configLoader native`). Vite otherwise bundles
  `vite.config.ts` to a temporary file, and `import.meta.dir`, which the
  config and `site.ts` use to find the repository, then points at that file.
- **Styled with Tailwind only, preflight included.** `styles.css` is
  `@import "tailwindcss"` and a theme: the page's two fonts and eight colors,
  given their dark values under `@variant dark`. Everything else is a
  utility class, on the components' elements (ADR 0044). The classes more
  than one component uses are constants in `styles.rs`. A file's delete button uses `group-hover`
  on its row. CodeMirror's `.cm-editor` is sized from its parent, with
  `[&_.cm-editor]:h-full`. Preflight resets native buttons and selects, so
  those now have a look of their own, from the theme's colors.
  - The old `.cm-scroller` rule is gone. CodeMirror's own theme always
    overrode it, and still overrides anything in Tailwind's layers.
  - The Result frame is another document, the user's program's page. It
    keeps its few lines of CSS.
- **One chunk.** CodeMirror, React and the WASI shim bundle to about 850 KB
  (280 KB gzipped), so the warning limit is 1 MB rather than splitting a page
  that needs all of it to start.

## Why

- **The same toolchain as users.** Fast Refresh on the components and React
  Compiler's output are now tested by the app we work on every day. Before,
  only the example tested them.
- **One way to compile Rust in Vite.** The WASI compiler plugs into the
  plugin rather than a second watcher, so the playground gets the plugin's
  batching, manifest-driven reloads and error overlay.

## Consequences

- Saving a file in `rust/` recompiles with `rust-js.wasm`. Once the crates
  are built, that's a second or two, slower than the native binary.
- In development the page loads through Vite's module graph, as in the
  example. The static site is what `test/playground.test.ts` checks.
- A browser tab still open on the old Bun dev server keeps retrying Bun's
  HMR socket on port 4400. Vite never answers it, and Chrome makes other
  sockets to that port, Vite's own included, wait behind it. Close old tabs.
