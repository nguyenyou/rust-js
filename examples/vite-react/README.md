# React + Vite, in Rust

This is create-vite's React template (`bun create vite --template react`,
create-vite 9.2.1: Vite 8, `@vitejs/plugin-react` 6, React 19), with
`src/App.jsx` written in Rust as [`src/App.rs`](src/App.rs). rust-js compiles
it to `src/App.jsx`, the component you'd have written by hand, and Vite
serves that with Fast Refresh. See [ADR 0041](../../docs/decisions/0041-react.md).

```bash
bun run build              # in the repository root, once: rust-js and the react crate
cd examples/vite-react
bun run dev                # http://localhost:5173; edit src/App.rs and save
bun run build              # dist/
```

```
save App.rs ─► vite-plugin-rust-js: rust-js ─► App.jsx ─► Vite HMR ─► Fast Refresh keeps state
```

A compile error shows in Vite's overlay, and the page keeps running the last
version that compiled. The browser's source map points at `App.rs`.

What changed from the template:

- `src/App.jsx` is now `src/App.rs`, and the generated `App.jsx` is in
  `.gitignore`.
- `vite.config.js` adds `rustJs()` before `react()`.
- `src/main.jsx` imports `{ App }`, because rust-js exports by name.
- `package.json` adds `vite-plugin-rust-js`.
