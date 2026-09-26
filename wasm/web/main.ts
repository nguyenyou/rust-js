// The playground is written in Rust (ADR 0032), with React (ADR 0044):
// rust/lib.rs, which build.ts and serve.ts compile to rust/lib.jsx with
// rust-js itself (compile-rust.ts). This file only starts it.

import { start } from "./rust/lib.jsx";

await start();
