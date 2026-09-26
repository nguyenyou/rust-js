// The playground, in Rust (ADR 0032): React components (ADR 0044), one per
// file in components/. rust-js compiles them to JSX beside them, running as
// WebAssembly, the same compiler the page runs (see ../compile-rust.ts), and
// main.ts calls `start`.

#![feature(extern_types)]
#![allow(non_snake_case)]

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
use react::{component, strict_mode};
use web::document;

use components::app::App;

/// Render the page into `#app`.
pub fn start() {
    let root = create_root(document::get_element_by_id(document, "app").expect("the page has an #app"));
    root.render(strict_mode(component(App, ())));
}
