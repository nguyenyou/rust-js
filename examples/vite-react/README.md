# React + Vite, in Rust

This is create-vite's React template (`bun create vite --template react`,
create-vite 9.2.1: Vite 8, `@vitejs/plugin-react` 6, React 19), with
`src/App.jsx` written in Rust as [`src/App.rs`](src/App.rs). rust-js compiles
it to `src/App.jsx`, the component you'd have written by hand, and Vite
serves that with Fast Refresh. It also has React Compiler and Tailwind CSS,
set up as their own guides do. See [ADR 0041](../../docs/decisions/0041-react.md).

```bash
bun run build              # in the repository root, once: rust-js and the react crate
cd examples/vite-react
bun run dev                # http://localhost:5173; edit src/App.rs and save
bun run build              # dist/
```

```
save App.rs ─► vite-plugin-rust-js: rust-js ─► App.jsx ─► React Compiler ─► Vite HMR ─► Fast Refresh keeps state
            └─► Tailwind reads App.rs's classes ─► index.css updates in place
```

A compile error shows in Vite's overlay, and the page keeps running the last
version that compiled. The browser's source map points at `App.rs`.

- **React Compiler** memoizes the component rust-js writes, as it would one
  written by hand. It's configured as create-vite's `react-compiler` template
  has it: `babel({ presets: [reactCompilerPreset()] })`.
- **Tailwind CSS** scans `.rs` files for class names, so `App.rs` uses its
  classes directly. As in any template, write each class name whole:
  `if even { "text-emerald-500" } else { "text-sky-500" }`, not a string built
  from pieces.

What changed from the template:

- `src/App.jsx` is now `src/App.rs`. The `App.jsx` that rust-js writes from it
  is committed, as ReScript recommends for its JS: diffs show what a change did
  to the output, and a checkout without rust-js still builds from it (with a
  warning). Its source map, `App.jsx.map`, is in `.gitignore`.
- `vite.config.js` adds `rustJs()` first, then React Compiler's Babel preset
  and `tailwindcss()`. `rustJs()` has to come before `tailwindcss()`, so that
  a save refreshes the page instead of reloading it.
- `src/index.css` starts with `@import "tailwindcss";`.
- `src/main.jsx` imports `{ App }`, because rust-js exports by name.
- `package.json` adds `vite-plugin-rust-js`, `tailwindcss` and `@tailwindcss/vite`,
  and React Compiler's `babel-plugin-react-compiler`, `@rolldown/plugin-babel`
  and `@babel/core`.
