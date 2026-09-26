// The playground, in Rust (ADR 0032): React components (ADR 0044), one per
// file in components/. rust-js compiles them to JSX beside them, running as
// WebAssembly, the same compiler the page runs (see ../compile-rust.ts), and
// main.ts calls `start`.

#![feature(extern_types)]
#![allow(non_snake_case)]
// Functions and fields are camelCase in JS, as React code names them:
// `use_dark_mode` is `useDarkMode`, a prop `on_open` is `onOpen` (ADR 0046).
#![rust_js::camel_case]

mod components;

// What the components use: loading and running the compiler, the Result
// frame's page, CodeMirror, and the crate being edited.
mod codemirror;
mod compiler;
mod dark_mode;
mod listen;
mod programs;
mod projects;
mod styles;
mod tree;

use react::dom::client::create_root;
use web::document;

use components::app::App;

/// Render the page into `#app`.
pub fn start() {
    let root = create_root(document::get_element_by_id(document, "app").expect("the page has an #app"));
    root.render(jsx! { <StrictMode><App /></StrictMode> });
}
