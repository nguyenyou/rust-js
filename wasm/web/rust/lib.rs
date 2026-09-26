// The playground's own code, in Rust. When the site is built, rust-js
// compiles it to lib.js beside it, running as WebAssembly, the same compiler
// the page runs (see ../compile-rust.ts), and main.ts imports it. More of
// main.ts moves here, a part at a time.
//
// So far: loading what the page needs, and the stats table.

#![feature(extern_types)]

use web::{
    Promise, Response, Uint8Array, WebAssemblyModule, array_buffer, document, html_table_element, html_table_row_element,
    node, response, uint8_array, web_assembly, window,
};

// `names_json` and `examples_json` are one JS method, `json`, typed for
// each file it reads: rustc warns that native code would see one symbol.
#[allow(clashing_extern_declarations)]
unsafe extern "Rust" {
    /// A file in the WASI shim's in-memory filesystem.
    pub type WasiFile;
    /// A JS `Map`: a directory's contents, for the WASI shim.
    pub type JsMap;

    #[link_name = "performance.now"]
    safe fn now() -> f64;
    #[link_name = "toFixed"]
    safe fn to_fixed(this: f64, digits: u32) -> String;
    #[link_name = "new @bjorn3/browser_wasi_shim#File"]
    safe fn new_file(data: &Uint8Array, options: &dyn std::any::Any) -> &'static WasiFile;
    #[link_name = "get data"]
    safe fn file_data(this: &WasiFile) -> &'static Uint8Array;
    #[link_name = "new Map"]
    safe fn new_map(entries: Vec<(String, &'static WasiFile)>) -> &'static JsMap;
    #[link_name = "json"]
    safe fn names_json(this: &Response) -> Promise<Vec<String>>;
    #[link_name = "json"]
    safe fn examples_json(this: &Response) -> Promise<Vec<Example>>;
}

/// An example program: its files, under `examples/<name>/`.
pub struct Example {
    pub name: String,
    pub title: String,
    pub root: String,
    pub files: Vec<String>,
}

/// What the page needs before it can compile anything.
pub struct Loaded {
    pub module: &'static WebAssemblyModule,
    pub sysroot: &'static JsMap,
    pub web_crate: &'static WasiFile,
    pub examples: Vec<Example>,
}

/// The WASI shim's file options. Only the shim reads them.
#[allow(dead_code)]
struct FileOptions {
    readonly: bool,
}

pub fn ms(t: f64) -> String {
    to_fixed(t, 0) + " ms"
}

pub fn mb(n: f64) -> String {
    to_fixed(n / 1048576.0, 1) + " MB"
}

/// A row of the stats table under the editors.
pub fn stat(label: &str, value: &str) {
    let stats = document::get_element_by_id(document, "stats").expect("the page has a #stats table");
    let row = html_table_element::insert_row(html_table_element::unchecked_from(stats));
    node::set_text_content(html_table_row_element::insert_cell(row), label);
    node::set_text_content(html_table_row_element::insert_cell(row), value);
}

/// Download the compiler, the sysroot, the web crate and the examples.
pub async fn load() -> Loaded {
    let start = now();
    // All four start here, together: a JS promise runs as soon as it's made
    // (ADR 0029). Awaiting them one by one below only collects the results.
    let module = load_compiler(start);
    let sysroot = load_sysroot(start);
    let web_crate = load_web_crate(start);
    let examples = load_examples();
    let loaded = Loaded { module: module.await, sysroot: sysroot.await, web_crate: web_crate.await, examples: examples.await };
    stat("ready after", &ms(now() - start));
    loaded
}

async fn load_compiler(start: f64) -> &'static WebAssemblyModule {
    let module = web_assembly::compile_streaming(window::fetch_with_str(window, "./rust-js.wasm")).await;
    stat("download + compile rust-js.wasm", &ms(now() - start));
    module
}

async fn load_sysroot(start: f64) -> &'static JsMap {
    let names = names_json(window::fetch_with_str(window, "./sysroot.json").await).await;
    // Every file's download starts before the first is awaited.
    let mut downloads = Vec::new();
    for name in names {
        downloads.push(load_sysroot_file(name));
    }
    let mut entries = Vec::new();
    let mut size = 0;
    for download in downloads {
        let entry = download.await;
        size += uint8_array::length(file_data(entry.1));
        entries.push(entry);
    }
    let count = entries.len().to_string();
    stat("download sysroot", &(ms(now() - start) + " (" + &count + " files, " + &mb(size as f64) + ")"));
    new_map(entries)
}

async fn load_sysroot_file(name: String) -> (String, &'static WasiFile) {
    let response = window::fetch_with_str(window, &("./sysroot/".to_string() + &name)).await;
    let bytes = uint8_array::new(response::array_buffer(response).await);
    (name, new_file(bytes, &FileOptions { readonly: true }))
}

async fn load_web_crate(start: f64) -> &'static WasiFile {
    let bytes = response::array_buffer(window::fetch_with_str(window, "./web/libweb.rmeta").await).await;
    stat("download web crate", &(ms(now() - start) + " (" + &mb(array_buffer::byte_length(bytes) as f64) + ")"));
    new_file(uint8_array::new(bytes), &FileOptions { readonly: true })
}

async fn load_examples() -> Vec<Example> {
    examples_json(window::fetch_with_str(window, "./examples.json").await).await
}
