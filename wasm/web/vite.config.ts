// The playground is a Vite app (ADR 0045), with React, React Compiler and
// Tailwind, set up as create-vite's react-compiler template and Tailwind's
// Vite guide have them. Its own Rust (rust/lib.rs) is compiled by
// rust-js.wasm, the compiler the page runs (compile-rust.ts), on start and
// on every save, through vite-plugin-rust-js.

import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import rustJs from "vite-plugin-rust-js";

import { compileRust } from "./compile-rust.ts";
import { playgroundFiles } from "./site.ts";

export default defineConfig({
  // Every URL the page uses is relative, so it works under any base path
  // (on GitHub Pages: /rust-js/).
  base: "./",
  server: { port: 4400, strictPort: true },
  // CodeMirror, React and the WASI shim are one chunk: the page needs all of
  // them before it can start.
  build: { chunkSizeWarningLimit: 1024 },
  plugins: [
    playgroundFiles(),
    rustJs({ crates: ["rust/lib.rs"], compile: compileRust }),
    react(),
    babel({ presets: [reactCompilerPreset()] }),
    tailwindcss(),
  ],
});
