# A Vite and React app, in Rust

`src/App.rs` is this app's component, written in Rust. rust-js compiles it
to `src/App.jsx`, the component you'd have written by hand, and Vite serves
that with Fast Refresh. Otherwise it's create-vite's React template (Vite 8,
`@vitejs/plugin-react`, React 19), with React Compiler and Tailwind CSS.

```bash
bun install      # or npm install
bun run dev      # or npm run dev: http://localhost:5173; edit src/App.rs and save
bun run build    # or npm run build: dist/
```

```
save App.rs ─► @rust-js/vite-plugin: rust-js ─► App.jsx ─► React Compiler ─► Vite HMR ─► Fast Refresh keeps state
            └─► Tailwind reads App.rs's classes ─► index.css updates in place
```

A compile error shows in Vite's overlay, and the page keeps running the last
version that compiled. The browser's source map points at `App.rs`.

- **rust-js runs with Rust's nightly it's built with**, rustup's: it needs
  rustc's own library, and checks the app for `wasm32-unknown-unknown`. If
  it says that nightly isn't installed, install it as `bun create @rust-js`
  said to, and as it says: `rustup toolchain install <nightly> --profile
  minimal --target wasm32-unknown-unknown`, about 500 MB, once.
- **Bun or Node.js** installs the app and runs Vite. Node.js must be Vite 8's,
  `^20.19.0` or `>=22.12.0`, with Bun too if it's installed: `bun run dev`
  runs Vite on it.
- **`src/App.jsx` is committed**, as ReScript recommends for its JS: a diff
  shows what a change did to the output, and a checkout without rust-js
  still builds from it. Its source map, `App.jsx.map`, isn't.
- **React Compiler** memoizes the component rust-js writes, as it would one
  written by hand: `babel({ presets: [reactCompilerPreset()] })` in
  `vite.config.js`.
- **Tailwind CSS** reads `.rs` files for class names, so `App.rs` uses its
  classes directly. Write each class name whole: `if even { "text-emerald-500" }
  else { "text-sky-500" }`, not a string built from pieces.
- `rustJs()` comes first in `vite.config.js`, before `tailwindcss()`, so that
  a save refreshes the page instead of reloading it.
