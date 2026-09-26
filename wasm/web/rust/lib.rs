// The playground, in Rust (ADR 0032), rendered by React (ADR 0044). When
// the site is built, rust-js compiles it to lib.jsx beside it, running as
// WebAssembly, the same compiler the page runs (see ../compile-rust.ts), and
// main.ts calls `start`.
//
// A Rust crate in (a few files), one JS file per module out (ADR 0019).
// rust-js.wasm runs on an in-memory WASI filesystem:
//
//   /in/lib.rs, /in/stats.rs, ...   the crate, from the Rust editor
//   /out/lib.js, /out/stats.js, ... what rust-js writes (plus .js.map files)
//   /sysroot/...                    the std metadata rustc type-checks against
//   /web/libweb.rmeta               the web crate's metadata (ADR 0024)
//
// Each compile gets a fresh instance of the (compiled once) module: rustc
// keeps global state, and a failed compile ends in a trap.

#![feature(extern_types)]
#![allow(non_snake_case)]

mod page;

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;

use react::component;
use react::dom::client::create_root;
use react::dom::flush_sync;

use web::{
    Element, Event, HtmlButtonElement, HtmlIFrameElement, HtmlSelectElement, JsError, JsObject, MediaQueryList, Promise,
    RegExp, Response, Uint8Array, WebAssemblyInstance, WebAssemblyMemory,
    WebAssemblyModule, array_buffer, css_style_declaration, document, element, event_target, html_element,
    html_button_element, html_i_frame_element, html_option_element, html_select_element, html_table_element,
    html_table_row_element, js_error, media_query_list, media_query_list_event, node, reg_exp, response, spawn,
    text_decoder, text_encoder, uint8_array,
    web_assembly, web_assembly_instance, web_assembly_memory, window,
};

// Some JS functions are declared more than once, typed for each use (`json`
// for each file it reads, `new Map` for what it holds): rustc warns that
// native code would see one symbol.
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

    // The rest of the WASI shim (@bjorn3/browser_wasi_shim), for `compile`.
    /// A file or a directory: what a directory's `Map` holds.
    pub type Inode;
    pub type WasiDirectory;
    pub type PreopenDirectory;
    /// One of WASI's file descriptors: stdin, stdout, a preopened directory.
    pub type Fd;
    pub type Wasi;

    #[link_name = "new @bjorn3/browser_wasi_shim#File"]
    safe fn new_empty_file(data: Vec<u8>) -> &'static WasiFile;
    #[link_name = "new @bjorn3/browser_wasi_shim#File"]
    safe fn new_plain_file(data: &Uint8Array) -> &'static WasiFile;
    #[link_name = "new @bjorn3/browser_wasi_shim#Directory"]
    safe fn new_directory(contents: &JsMap) -> &'static WasiDirectory;
    #[link_name = "get contents"]
    safe fn contents(this: &WasiDirectory) -> &'static JsMap;
    #[link_name = "this"]
    safe fn file_inode(this: &WasiFile) -> &'static Inode;
    #[link_name = "this"]
    safe fn directory_inode(this: &WasiDirectory) -> &'static Inode;
    #[link_name = "instanceof @bjorn3/browser_wasi_shim#Directory"]
    safe fn is_directory(this: &Inode) -> bool;
    #[link_name = "instanceof @bjorn3/browser_wasi_shim#File"]
    safe fn is_file(this: &Inode) -> bool;
    #[link_name = "this"]
    safe fn as_directory(this: &Inode) -> &'static WasiDirectory;
    #[link_name = "this"]
    safe fn as_file(this: &Inode) -> &'static WasiFile;

    #[link_name = "new Map"]
    safe fn new_inode_map(entries: Vec<(String, &'static Inode)>) -> &'static JsMap;
    #[link_name = "new Map"]
    safe fn new_text_map(entries: Vec<(String, String)>) -> &'static JsMap;
    #[link_name = "Array.from"]
    safe fn inode_entries(map: &JsMap) -> Vec<(String, &'static Inode)>;
    #[link_name = "Array.from"]
    safe fn text_entries(map: &JsMap) -> Vec<(String, String)>;
    #[link_name = "has"]
    safe fn map_has(this: &JsMap, key: &str) -> bool;
    #[link_name = "get"]
    safe fn map_get(this: &JsMap, key: &str) -> &'static Inode;
    #[link_name = "set"]
    safe fn map_set(this: &JsMap, key: &str, value: &Inode);

    #[link_name = "new @bjorn3/browser_wasi_shim#OpenFile"]
    safe fn new_open_file(file: &WasiFile) -> &'static Fd;
    #[link_name = "@bjorn3/browser_wasi_shim#ConsoleStdout.lineBuffered"]
    safe fn line_buffered(write: Box<dyn FnMut(String)>) -> &'static Fd;
    #[link_name = "new @bjorn3/browser_wasi_shim#PreopenDirectory"]
    safe fn new_preopen(name: &str, contents: &JsMap) -> &'static PreopenDirectory;
    #[link_name = "this"]
    safe fn preopen_fd(this: &PreopenDirectory) -> &'static Fd;
    #[link_name = "get dir"]
    safe fn preopen_dir(this: &PreopenDirectory) -> &'static WasiDirectory;
    #[link_name = "new @bjorn3/browser_wasi_shim#WASI"]
    safe fn new_wasi(args: Vec<String>, env: Vec<String>, fds: Vec<&'static Fd>, options: &dyn std::any::Any) -> &'static Wasi;
    #[link_name = "get wasiImport"]
    safe fn wasi_import(this: &Wasi) -> &'static JsObject;
    /// Runs the program. A failed compile ends in a trap: panics can't unwind
    /// on wasm32-wasip1.
    #[link_name = "start"]
    safe fn run_wasi(this: &Wasi, instance: &WebAssemblyInstance) -> Result<i32, &'static JsError>;
    #[link_name = "get memory"]
    safe fn exported_memory(this: &JsObject) -> &'static WebAssemblyMemory;
    #[link_name = "instanceof Error"]
    safe fn is_error(this: &JsError) -> bool;
    #[link_name = "get message"]
    safe fn error_message(this: &JsError) -> String;
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
    format!("{} ms", to_fixed(t, 0))
}

pub fn mb(n: f64) -> String {
    format!("{} MB", to_fixed(n / 1048576.0, 1))
}

/// A row of the stats table under the editors.
pub fn stat(label: &str, value: &str) {
    let stats = document::get_element_by_id(document, "stats").expect("the page has a #stats table");
    let row = html_table_element::insert_row(html_table_element::unchecked_from(stats));
    let name = html_table_row_element::insert_cell(row);
    element::set_class_name(name, "py-0.5 pr-4 tabular-nums text-muted");
    node::set_text_content(name, label);
    let number = html_table_row_element::insert_cell(row);
    element::set_class_name(number, "py-0.5 pr-4 tabular-nums");
    node::set_text_content(number, value);
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
    stat("download sysroot", &format!("{} ({} files, {})", ms(now() - start), entries.len(), mb(size as f64)));
    new_map(entries)
}

async fn load_sysroot_file(name: String) -> (String, &'static WasiFile) {
    let response = window::fetch_with_str(window, &format!("./sysroot/{name}")).await;
    let bytes = uint8_array::new(response::array_buffer(response).await);
    (name, new_file(bytes, &FileOptions { readonly: true }))
}

async fn load_web_crate(start: f64) -> &'static WasiFile {
    let bytes = response::array_buffer(window::fetch_with_str(window, "./web/libweb.rmeta").await).await;
    stat("download web crate", &format!("{} ({})", ms(now() - start), mb(array_buffer::byte_length(bytes) as f64)));
    new_file(uint8_array::new(bytes), &FileOptions { readonly: true })
}

async fn load_examples() -> Vec<Example> {
    examples_json(window::fetch_with_str(window, "./examples.json").await).await
}

// ── File explorer ───────────────────────────────────────────────────────

/// A folder's entries, in the order they came: subfolders, or a file's full path.
type Tree = Vec<(String, Entry)>;

enum Entry {
    Folder(Tree),
    File(String),
}

fn build_tree(paths: &[String]) -> Tree {
    let mut tree: Tree = Vec::new();
    for path in paths {
        let mut folder = &mut tree;
        let name = match path.rsplit_once('/') {
            Some((folders, name)) => {
                for part in folders.split('/') {
                    if !folder.iter().any(|(n, _)| n == part) {
                        folder.push((part.to_string(), Entry::Folder(Vec::new())));
                    }
                    folder = match folder.iter_mut().find(|(n, _)| n == part) {
                        Some((_, Entry::Folder(children))) => children,
                        _ => unreachable!("a folder, found or just made"),
                    };
                }
                name
            }
            None => path.as_str(),
        };
        folder.push((name.to_string(), Entry::File(path.clone())));
    }
    tree
}

/// What `render_tree` shows, and what clicking a file does.
pub struct TreeOptions {
    pub selected: String,
    pub first: Option<String>,
    pub on_open: Rc<dyn Fn(String)>,
    pub decorate: Option<Rc<dyn Fn(&Element, String)>>,
}

/// Render a file tree into `list`: a button per file, folders as labels.
pub fn render_tree(list: &Element, paths: Vec<String>, options: TreeOptions) {
    node::set_text_content(list, "");
    render(&build_tree(&paths), list, 0, &options);
}

fn render(tree: &Tree, into: &Element, depth: u32, options: &TreeOptions) {
    // The crate root first; then by name, a module's file just before its
    // folder: `geometry.rs`, then `geometry/` ("." sorts before "/").
    let key = |(name, entry): &(String, Entry)| match entry {
        Entry::Folder(_) => format!("{name}/"),
        Entry::File(_) => name.clone(),
    };
    let is_first = |entry: &Entry| matches!((entry, &options.first), (Entry::File(path), Some(first)) if path == first);
    let mut entries: Vec<&(String, Entry)> = tree.iter().collect();
    entries.sort_by(|a, b| {
        if is_first(&a.1) {
            Ordering::Less
        } else if is_first(&b.1) {
            Ordering::Greater
        } else {
            key(a).cmp(&key(b))
        }
    });
    for (name, entry) in entries {
        let li = document::create_element(document, "li");
        // `group`: a file's delete button shows while its row is hovered.
        element::set_class_name(li, "group flex items-center");
        let indent = format!("{}px", 8 + depth * 12);
        match entry {
            Entry::Folder(children) => {
                let label = html_element::unchecked_from(document::create_element(document, "span"));
                element::set_class_name(label, "block px-2 py-0.5 text-muted");
                css_style_declaration::set_property(html_element::style(label), "padding-left", &indent);
                node::set_text_content(label, &format!("{name}/"));
                let nested = document::create_element(document, "ul");
                render(children, nested, depth + 1, options);
                let wrapper = document::create_element(document, "div");
                element::set_class_name(wrapper, "w-full");
                element::append(wrapper, label);
                element::append(wrapper, nested);
                element::append(li, wrapper);
            }
            Entry::File(path) => {
                let file = html_element::unchecked_from(document::create_element(document, "button"));
                element::set_class_name(file, "min-w-0 flex-1 cursor-pointer truncate px-2 py-0.5 text-left aria-[current=true]:bg-selected");
                css_style_declaration::set_property(html_element::style(file), "padding-left", &indent);
                node::set_text_content(file, name);
                element::set_attribute(file, "aria-current", &(*path == options.selected).to_string());
                let on_open = options.on_open.clone();
                let opened = path.clone();
                event_target::add_event_listener(file, "click", Box::new(move |_| on_open(opened.clone())));
                element::append(li, file);
                if let Some(decorate) = &options.decorate {
                    decorate(li, path.clone());
                }
            }
        }
        element::append(into, li);
    }
}

// ── Running rust-js ─────────────────────────────────────────────────────

/// What a compile gives back. `exit` is its code, or how it trapped.
pub struct Compiled {
    pub exit: String,
    pub ok: bool,
    /// The JS files, `path → text`.
    pub files: &'static JsMap,
    pub stderr: String,
    pub instantiate: f64,
    pub run: f64,
    pub memory: u32,
}

/// The WASI shim's options, and the module's imports. Only JS reads them.
#[allow(dead_code)]
struct WasiOptions {
    debug: bool,
}

#[allow(dead_code)]
struct Imports {
    wasi_snapshot_preview1: &'static JsObject,
}

/// A directory holding one entry.
fn dir(name: &str, entry: &'static Inode) -> &'static Inode {
    directory_inode(new_directory(new_inode_map(vec![(name.to_string(), entry)])))
}

/// A WASI directory tree from `path → text`, e.g. `geometry/area.rs`.
fn directory_of(sources: &JsMap) -> &'static JsMap {
    let top = new_inode_map(Vec::new());
    for (path, text) in text_entries(sources) {
        let mut folder = top;
        let name = match path.rsplit_once('/') {
            Some((folders, name)) => {
                for part in folders.split('/') {
                    if !map_has(folder, part) {
                        map_set(folder, part, directory_inode(new_directory(new_inode_map(Vec::new()))));
                    }
                    folder = contents(as_directory(map_get(folder, part)));
                }
                name
            }
            None => path.as_str(),
        };
        let bytes = text_encoder::encode_with_input(text_encoder::new(), &text);
        map_set(folder, name, file_inode(new_plain_file(bytes)));
    }
    top
}

/// Every `.js` file under a WASI directory, as `path → text`.
fn js_files_in(folder: &WasiDirectory, prefix: &str, found: &mut Vec<(String, String)>) {
    for (name, entry) in inode_entries(contents(folder)) {
        if is_directory(entry) {
            js_files_in(as_directory(entry), &format!("{prefix}{name}/"), found);
        } else if is_file(entry) && name.ends_with(".js") {
            let text = text_decoder::decode_with_uint8_array(text_decoder::new(), file_data(as_file(entry)));
            found.push((format!("{prefix}{name}"), text));
        }
    }
}

/// Run rust-js.wasm on the crate in `sources` (`path → text`): a fresh
/// instance each time, since rustc keeps global state, and a failed compile
/// ends in a trap. With `test`, the crate's `#[test]` functions too (ADR 0026).
pub async fn compile(
    module: &WebAssemblyModule,
    sysroot: &JsMap,
    web_crate: &WasiFile,
    sources: &JsMap,
    root_file: &str,
    test: bool,
) -> Compiled {
    let stderr = Rc::new(RefCell::new(Vec::new()));
    let stdout_lines = stderr.clone();
    let stderr_lines = stderr.clone();
    let out_dir = new_preopen("/out", new_inode_map(Vec::new()));
    let sysroot_dir = directory_inode(new_directory(sysroot));
    let fds = vec![
        new_open_file(new_empty_file(Vec::new())), // stdin
        line_buffered(Box::new(move |line| stdout_lines.borrow_mut().push(line))), // stdout
        line_buffered(Box::new(move |line| stderr_lines.borrow_mut().push(line))), // stderr
        preopen_fd(new_preopen("/in", directory_of(sources))),
        preopen_fd(out_dir),
        preopen_fd(new_preopen(
            "/sysroot",
            new_inode_map(vec![("lib".to_string(), dir("rustlib", dir("wasm32-unknown-unknown", dir("lib", sysroot_dir))))]),
        )),
        preopen_fd(new_preopen("/web", new_inode_map(vec![("libweb.rmeta".to_string(), file_inode(web_crate))]))),
    ];
    let out_file = match root_file.strip_suffix(".rs") {
        Some(stem) => format!("/out/{stem}.js"),
        None => format!("/out/{root_file}"),
    };
    // `--test`: the `#[test]` functions too, and `<root>.test.js` to run them
    // (ADR 0026). This is a real browser, so tests marked `#[cfg(browser)]` run
    // too (ADR 0027). Every program may use the web crate; rustc only reads
    // it if one does.
    let mut args = vec!["rust-js".to_string()];
    if test {
        args.push("--test".to_string());
    }
    for arg in [format!("/in/{root_file}"), "-o".to_string(), out_file] {
        args.push(arg);
    }
    for arg in ["--", "--target", "wasm32-unknown-unknown", "--sysroot", "/sysroot"] {
        args.push(arg.to_string());
    }
    if test {
        args.push("--cfg=browser".to_string());
    }
    args.push("--extern".to_string());
    args.push("web=/web/libweb.rmeta".to_string());
    // RUSTC_ICE=0: don't name a crash-report file after the process id (WASI
    // has none). Without options, the shim logs every call it handles.
    let wasi = new_wasi(args, vec!["RUSTC_ICE=0".to_string()], fds, &WasiOptions { debug: false });

    let t0 = now();
    let imports = Imports { wasi_snapshot_preview1: wasi_import(wasi) };
    let instance = web_assembly::instantiate_with_web_assembly_module_and_import_object(module, &imports).await;
    let t1 = now();
    let started = run_wasi(wasi, instance);
    let t2 = now();
    let ok = matches!(started, Ok(0));
    let exit = match started {
        Ok(code) => code.to_string(),
        Err(e) => format!("trap ({})", if is_error(e) { error_message(e) } else { js_error::to_string(e) }),
    };

    let mut files = Vec::new();
    if ok {
        js_files_in(preopen_dir(out_dir), "", &mut files);
    }
    let memory = web_assembly_memory::buffer(exported_memory(web_assembly_instance::exports(instance)));
    Compiled {
        exit,
        ok,
        files: new_text_map(files),
        stderr: stderr.borrow().join("\n"),
        instantiate: t1 - t0,
        run: t2 - t1,
        memory: array_buffer::byte_length(memory),
    }
}

// ── Linking the program ─────────────────────────────────────────────────

// `replace` is one JS method, typed for each way it's called.
#[allow(clashing_extern_declarations)]
unsafe extern "Rust" {
    #[link_name = "JSON.stringify"]
    safe fn json_string(text: &str) -> String;
    /// `text.replace(pattern, (match, a, b) => ..)`: a closure for each match.
    #[link_name = "replace"]
    safe fn replace_matches(this: &str, pattern: &RegExp, with: Box<dyn Fn(String, String, String) -> String>) -> String;
    #[link_name = "replace"]
    safe fn replace_pattern(this: &str, pattern: &RegExp, with: &str) -> String;
}

/// `from`'s directory joined with a relative specifier like `../lib.js`.
pub fn resolve(from: &str, specifier: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in specifier.split('/') {
        if part == ".." {
            parts.pop();
        } else if part != "." {
            parts.push(part);
        }
    }
    parts.join("/")
}

/// rust-js's modules (ADR 0019) as one classic script. Each module becomes a
/// function that fills in its exports object, and `import * as util from
/// "./util.js"` becomes that module's exports object. The objects all exist
/// before any module runs, so cycles work: functions are only called later.
pub fn link(files: &JsMap, start: &str) -> String {
    let files = text_entries(files);
    let mut parts = vec!["const modules = {};".to_string()];
    for (path, _) in &files {
        parts.push(format!("modules[{}] = {{}};", json_string(path)));
    }
    // A test file reads the tests' functions as it registers them, so it goes
    // after the modules that define them.
    let mut ordered: Vec<&(String, String)> = files.iter().collect();
    ordered.sort_by_key(|(path, _)| path.ends_with(".test.js"));
    let imports = reg_exp::new(r#"^import \* as (\S+) from "([^"]+)";$"#, "gm");
    // What a module exports: its functions, async ones, and constants (ADRs 0019, 0029, 0031).
    let exports = reg_exp::new(r"^export (async function|function|const) (\w+)", "gm");
    let source_map = reg_exp::new(r"^//# sourceMappingURL=.*$", "m");
    for (path, code) in ordered {
        let from = path.clone();
        let body = replace_matches(
            code,
            imports,
            Box::new(move |_, alias, specifier| format!("const {alias} = modules[{}];", json_string(&resolve(&from, &specifier)))),
        );
        let exported = Rc::new(RefCell::new(Vec::new()));
        let names = exported.clone();
        let body = replace_matches(
            &body,
            exports,
            Box::new(move |_, declared, name| {
                names.borrow_mut().push(name.clone());
                format!("{declared} {name}")
            }),
        );
        let body = replace_pattern(&body, source_map, "");
        let names = exported.borrow().join(", ");
        parts.push(format!("(function (exports) {{\n{body}\nObject.assign(exports, {{ {names} }});\n}})(modules[{}]);", json_string(path)));
    }
    parts.push(start.to_string());
    // A `</script>` in a string would end the script early; `<\/script>` is the same string.
    parts.join("\n").replace("</script", "<\\/script")
}

// ── The status line ─────────────────────────────────────────────────────

/// The line beside the buttons. `kind` is `""`, `"good"` or `"bad"`.
pub fn set_status(text: &str, kind: &str) {
    let status = document::get_element_by_id(document, "status").expect("the page has a #status");
    node::set_text_content(status, text);
    // Not a `match` on the strings: rust-js can't compile string patterns yet.
    let color = if kind == "good" {
        "text-good"
    } else if kind == "bad" {
        "text-bad"
    } else {
        ""
    };
    element::set_class_name(status, color);
}

fn status_text() -> String {
    let status = document::get_element_by_id(document, "status").expect("the page has a #status");
    node::text_content(status).unwrap_or(String::new())
}

// ── Running the program ─────────────────────────────────────────────────
// If the root module exports `main`, run it in a frame with a
// `<div id="app">` to render into. The modules are linked into one plain
// `<script>` (see `link`), which every browser runs the same way, and the
// page reports back whether `main` ran, so the status line always says.
//
// The frame isn't sandboxed. Chrome runs a sandboxed frame in a process of
// its own, and some setups then don't draw it until something else changes
// the layout: the program ran, but the frame stayed blank. The program is
// the one in the editor, so it may share this page's origin.

// What the page keeps between runs (ADR 0037).
thread_local! {
    /// How many programs have run: a report from an older one is ignored.
    static PROGRAM_RUNS: Cell<u32> = Cell::new(0);
    /// Whether the latest one reported back.
    static REPORTED: Cell<bool> = Cell::new(false);
    /// The Result frame: a new one for each run. Found on first use, since
    /// React renders it after this module loads.
    static RESULT_FRAME: Cell<Option<&'static HtmlIFrameElement>> = Cell::new(None);
}

// Some JS functions are declared more than once, typed for each use.
#[allow(clashing_extern_declarations)]
unsafe extern "Rust" {
    #[link_name = "setTimeout"]
    safe fn set_timeout(callback: Box<dyn FnOnce()>, ms: u32);
    #[link_name = "matchAll"]
    safe fn match_all(this: &str, pattern: &RegExp) -> &'static JsObject;
    /// Each match of a pattern with one group, as `(match, group)`.
    #[link_name = "Array.from"]
    safe fn matches_of(matches: &JsObject) -> Vec<(String, String)>;
    #[link_name = "get contentWindow"]
    safe fn content_window(this: &HtmlIFrameElement) -> Option<&'static JsObject>;
    #[link_name = "get source"]
    safe fn message_source(this: &Event) -> Option<&'static JsObject>;
    #[link_name = "get data"]
    safe fn message_data(this: &Event) -> Option<Report>;
    #[link_name = "Object.is"]
    safe fn same_object(a: Option<&JsObject>, b: Option<&JsObject>) -> bool;
}

fn result_frame() -> &'static HtmlIFrameElement {
    match RESULT_FRAME.get() {
        Some(frame) => frame,
        None => frame_by_id("result"),
    }
}

fn frame_by_id(id: &str) -> &'static HtmlIFrameElement {
    html_i_frame_element::unchecked_from(document::get_element_by_id(document, id).expect("the page has the Result frame"))
}

/// What the Result frame posts back. Fields it doesn't send are `undefined`.
pub struct Report {
    pub run: Option<u32>,
    pub error: Option<String>,
    pub ran: Option<bool>,
    pub tested: Option<Tested>,
}

pub struct Tested {
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
}

/// A small `bun test` look-alike for the Result frame: `test` and `test.skip`
/// collect the tests, which then run one after another. What they leave in
/// the page is replaced by the report.
const TEST_RUNNER: &str = r#"
    const results = registered.map(({ name, f }) => {
      if (!f) return { name, outcome: "skip" };
      try {
        f();
        return { name, outcome: "pass" };
      } catch (e) {
        return { name, outcome: "fail", message: e instanceof Error ? e.message : String(e) };
      }
    });
    document.body.replaceChildren(...results.map(({ name, outcome, message }) => {
      const line = document.createElement("div");
      line.className = outcome;
      line.textContent = { pass: "✓ ", fail: "✗ ", skip: "– " }[outcome] + name + (outcome === "skip" ? " (ignored)" : "");
      if (message) {
        const why = document.createElement("pre");
        why.textContent = message;
        line.append(why);
      }
      return line;
    }));
    const count = (outcome) => results.filter((r) => r.outcome === outcome).length;"#;

/// The frame's style, before its scripts.
const FRAME_HEAD: &str = r#"<!doctype html>
<meta charset="utf-8">
<style>
  :root { color-scheme: light dark; font: 15px/1.5 system-ui, sans-serif; }
  body { margin: 12px; }
  button { font: inherit; min-width: 2.5em; padding: 2px 10px; }
  output { display: inline-block; min-width: 3em; text-align: center; font-variant-numeric: tabular-nums; }
  .pass { color: #2f6b3a; } .fail { color: #a3321f; } .skip { color: #6b6b66; }
  @media (prefers-color-scheme: dark) { .pass { color: #8fcf98; } .fail { color: #ef8a78; } }
  pre { margin: 2px 0 8px 1.5em; white-space: pre-wrap; font-size: 13px; }
</style>
<div id="app"></div>"#;

/// Run the root module's `main()`, or with `test`, the crate's tests.
pub fn run_program(files: &JsMap, root_file: &str, test: bool) {
    let sources = text_entries(files);
    let tests = match root_file.strip_suffix(".js") {
        Some(stem) => format!("{stem}.test.js"),
        None => root_file.to_string(),
    };
    PROGRAM_RUNS.set(PROGRAM_RUNS.get() + 1);
    // `main`, sync or async (ADR 0029).
    let has_main = reg_exp::new(r"^export (async )?function main\(\)", "m");
    let runnable = if test {
        sources.iter().any(|(path, _)| *path == tests)
    } else {
        sources.iter().any(|(path, code)| path == root_file && reg_exp::test(has_main, code))
    };
    // Imports from JS modules (ADR 0028) name packages or files the page
    // doesn't have. A bundler would bring them in; the playground has none.
    let imports = reg_exp::new(r#"^import .* from "([^"]+)";$"#, "gm");
    let mut external: Vec<String> = Vec::new();
    for (path, code) in &sources {
        for (_, specifier) in matches_of(match_all(code, imports)) {
            let target = resolve(path, &specifier);
            if !sources.iter().any(|(p, _)| *p == target) && !external.contains(&specifier) {
                external.push(specifier);
            }
        }
    }
    let section = html_element::unchecked_from(document::get_element_by_id(document, "result-section").expect("the page has a #result-section"));
    if !runnable || !external.is_empty() {
        html_element::set_hidden(section, true);
        html_i_frame_element::set_srcdoc(result_frame(), "");
        if runnable {
            let names: Vec<String> = external.iter().map(|s| format!("\"{s}\"")).collect();
            let text = format!(
                "{} Not run: it imports {}, which the playground can't load. Bundle it with bun build.",
                status_text(),
                names.join(", ")
            );
            set_status(&text, "bad");
        }
        return;
    }
    let run = PROGRAM_RUNS.get();
    let report = |message: &str| format!("parent.postMessage({{ run: {run}, {message} }}, \"*\")");
    REPORTED.set(false);
    // If the page never reports, say so: something stopped its script.
    set_timeout(
        Box::new(move || {
            if run == PROGRAM_RUNS.get() && !REPORTED.get() {
                set_status("The Result frame didn't run. Is something blocking its script? See the console.", "bad");
            }
        }),
        3000,
    );
    html_element::set_hidden(section, false);
    // A new frame each run: the program starts from a clean page, and a frame
    // made while its section is showing gets drawn right away.
    let frame = html_i_frame_element::unchecked_from(node::clone_node(result_frame()));
    element::replace_with(result_frame(), frame);
    RESULT_FRAME.set(Some(frame));
    let linked = if test { link(files, TEST_RUNNER) } else { link(files, &format!("modules[{}].main();", json_string(root_file))) };
    let finished = if test {
        report(r#"tested: { passed: count("pass"), failed: count("fail"), ignored: count("skip") }"#)
    } else {
        report("ran: true")
    };
    let page = format!(
        r#"{FRAME_HEAD}
<script>
  // Errors later on, in an event handler say.
  addEventListener("error", (e) => {});
  // And in async code, which rejects its promise instead (ADR 0029).
  addEventListener("unhandledrejection", (e) => {});
  // What a test file calls, as bun test provides it (ADR 0026).
  const registered = [];
  globalThis.test = (name, f) => registered.push({{ name, f }});
  test.skip = (name) => registered.push({{ name }});
</script>
<script>
  try {{
{linked}
    {finished};
  }} catch (e) {{
    {};
  }}
</script>"#,
        report("error: String(e.message)"),
        report("error: String(e.reason)"),
        report("error: String(e)"),
    );
    html_i_frame_element::set_srcdoc(frame, &page);
}

/// Listen for the Result frame's reports, and say what they say.
pub fn listen_for_reports() {
    event_target::add_event_listener(window, "message", Box::new(|e| {
        let from_frame = same_object(message_source(e), content_window(result_frame()));
        let report = match message_data(e) {
            Some(report) if from_frame && report.run == Some(PROGRAM_RUNS.get()) => report,
            _ => return,
        };
        REPORTED.set(true);
        if let Some(error) = report.error {
            set_status(&format!("Runtime error: {error}"), "bad");
        } else if report.ran == Some(true) {
            set_status(&format!("{} Ran main().", status_text()), "good");
        } else if let Some(tested) = report.tested {
            let total = tested.passed + tested.failed;
            let ignored = if tested.ignored > 0 { format!(", {} ignored", tested.ignored) } else { String::new() };
            let summary = if total == 0 {
                "No tests.".to_string()
            } else {
                format!("Tests: {} passed, {} failed{ignored}.", tested.passed, tested.failed)
            };
            set_status(&summary, if tested.failed > 0 { "bad" } else { "good" });
        }
    }));
}

// ── Editors ─────────────────────────────────────────────────────────────
// Rust in, JavaScript out, both CodeMirror, through bindings (ADR 0028).
// Both follow the system's light or dark setting.

#[allow(clashing_extern_declarations)]
unsafe extern "Rust" {
    /// Anything CodeMirror takes as an extension, an array of them too.
    pub type Extension;
    pub type Compartment;
    /// A change to an editor's configuration: a compartment's new contents.
    pub type Effect;
    pub type EditorState;
    pub type EditorView;
    /// A document's text.
    pub type Text;

    #[link_name = "codemirror#basicSetup"]
    safe static basic_setup: &'static Extension;
    #[link_name = "@codemirror/theme-one-dark#oneDark"]
    safe static one_dark: &'static Extension;
    #[link_name = "@codemirror/lang-rust#rust"]
    safe fn rust_language() -> &'static Extension;
    #[link_name = "@codemirror/lang-javascript#javascript"]
    safe fn javascript_language() -> &'static Extension;
    /// Extensions together are one.
    #[link_name = "this"]
    safe fn together(this: Vec<&'static Extension>) -> &'static Extension;
    #[link_name = "new @codemirror/state#Compartment"]
    safe fn new_compartment() -> &'static Compartment;
    #[link_name = "of"]
    safe fn compartment_of(this: &Compartment, content: &Extension) -> &'static Extension;
    #[link_name = "reconfigure"]
    safe fn reconfigure(this: &Compartment, content: &Extension) -> &'static Effect;
    #[link_name = "@codemirror/state#Prec.highest"]
    safe fn highest(extension: &Extension) -> &'static Extension;
    #[link_name = "@codemirror/view#keymap.of"]
    safe fn keymap_of(bindings: Vec<KeyBinding>) -> &'static Extension;
    #[link_name = "codemirror#EditorView.contentAttributes.of"]
    safe fn content_attributes(attributes: &JsObject) -> &'static Extension;
    #[link_name = "@codemirror/state#EditorState.readOnly.of"]
    safe fn read_only(value: bool) -> &'static Extension;
    #[link_name = "Object.fromEntries"]
    safe fn object_of(entries: Vec<(String, String)>) -> &'static JsObject;
    #[link_name = "@codemirror/state#EditorState.create"]
    safe fn create_state(config: &StateConfig) -> &'static EditorState;
    #[link_name = "new codemirror#EditorView"]
    safe fn new_editor(config: &EditorConfig) -> &'static EditorView;
    #[link_name = "get state"]
    safe fn editor_state(this: &EditorView) -> &'static EditorState;
    #[link_name = "setState"]
    safe fn set_editor_state(this: &EditorView, state: &EditorState);
    /// The editor's element, to put on the page.
    #[link_name = "get dom"]
    safe fn editor_dom(this: &EditorView) -> &'static Element;
    #[link_name = "dispatch"]
    safe fn dispatch(this: &EditorView, transaction: &Transaction);
    #[link_name = "get doc"]
    safe fn doc(this: &EditorState) -> &'static Text;
    #[link_name = "get length"]
    safe fn text_length(this: &Text) -> u32;
    #[link_name = "toString"]
    safe fn text_string(this: &Text) -> String;
    #[link_name = "set lastResult"]
    safe fn set_last_result(this: &web::Window, result: &Compiled);
}

// CodeMirror's configurations: JS objects that only CodeMirror reads.
#[allow(dead_code)]
pub struct KeyBinding {
    key: String,
    run: Box<dyn Fn() -> bool>,
}

#[allow(dead_code)]
struct StateConfig {
    doc: String,
    extensions: &'static Extension,
}

#[allow(dead_code)]
struct EditorConfig {
    parent: &'static Element,
    extensions: &'static Extension,
}

#[allow(dead_code)]
struct Transaction {
    changes: Option<Change>,
    effects: Option<&'static Effect>,
}

#[allow(dead_code)]
struct Change {
    from: u32,
    to: u32,
    insert: String,
}

fn by_id(id: &str) -> &'static Element {
    document::get_element_by_id(document, id).expect("the page has this element")
}

fn button_by_id(id: &str) -> &'static HtmlButtonElement {
    html_button_element::unchecked_from(by_id(id))
}

fn theme_for(dark: bool) -> &'static Extension {
    if dark { one_dark } else { together(Vec::new()) }
}

fn is_dark() -> bool {
    media_query_list::matches(DARK_MODE.get())
}

/// A change to a compartment: a new theme, or a new language.
fn effect(effect: &'static Effect) -> Transaction {
    Transaction { changes: None, effects: Some(effect) }
}

/// Replace an editor's whole text.
fn replace_text(editor: &EditorView, text: &str, effects: Option<&'static Effect>) {
    let to = text_length(doc(editor_state(editor)));
    dispatch(editor, &Transaction { changes: Some(Change { from: 0, to, insert: text.to_string() }), effects });
}

fn source_extensions() -> &'static Extension {
    together(vec![
        basic_setup,
        rust_language(),
        compartment_of(SOURCE_THEME.get(), theme_for(is_dark())),
        // basicSetup binds Mod-Enter to "insert blank line": outrank it.
        highest(keymap_of(vec![KeyBinding {
            key: "Mod-Enter".to_string(),
            run: Box::new(|| {
                spawn(Box::new(on_compile(false)));
                true
            }),
        }])),
        content_attributes(object_of(vec![("aria-label".to_string(), "Rust source".to_string())])),
    ])
}

/// Read-only, but still selectable and copyable. Highlighted as JS for a
/// generated file, plain text when it shows rustc's diagnostics.
fn output_extensions() -> &'static Extension {
    together(vec![
        basic_setup,
        compartment_of(OUTPUT_LANGUAGE.get(), javascript_language()),
        compartment_of(OUTPUT_THEME.get(), theme_for(is_dark())),
        read_only(true),
        content_attributes(object_of(vec![("aria-label".to_string(), "Generated JavaScript".to_string())])),
    ])
}

// What the page keeps, as the module loads (ADR 0037).
thread_local! {
    static DARK_MODE: Cell<&'static MediaQueryList> = Cell::new(window::match_media(window, "(prefers-color-scheme: dark)"));
    static SOURCE_THEME: Cell<&'static Compartment> = Cell::new(new_compartment());
    static OUTPUT_THEME: Cell<&'static Compartment> = Cell::new(new_compartment());
    static OUTPUT_LANGUAGE: Cell<&'static Compartment> = Cell::new(new_compartment());
    static SOURCE_EXTENSIONS: Cell<&'static Extension> = Cell::new(source_extensions());
    // Made before React renders where they go: `start` moves them in.
    static SOURCE: Cell<&'static EditorView> =
        Cell::new(new_editor(&EditorConfig { parent: detached(), extensions: SOURCE_EXTENSIONS.get() }));
    static OUTPUT: Cell<&'static EditorView> =
        Cell::new(new_editor(&EditorConfig { parent: detached(), extensions: output_extensions() }));

    // The crate being edited. Each file keeps its own editor state, so undo
    // history survives switching. In the order they came, as a JS `Map`.
    static ROOT: RefCell<String> = RefCell::new("lib.rs".to_string());
    static FILES: RefCell<Vec<(String, &'static EditorState)>> = RefCell::new(Vec::new());
    static CURRENT: RefCell<String> = RefCell::new(String::new());

    // What the last compile wrote, and which of it is showing.
    static OUTPUTS: RefCell<Vec<(String, String)>> = RefCell::new(Vec::new());
    static SHOWN_OUTPUT: RefCell<String> = RefCell::new(String::new());

    // What `load` brought, and the compiles so far.
    static MODULE: Cell<Option<&'static WebAssemblyModule>> = Cell::new(None);
    static SYSROOT: Cell<Option<&'static JsMap>> = Cell::new(None);
    static WEB_CRATE: Cell<Option<&'static WasiFile>> = Cell::new(None);
    static EXAMPLES: RefCell<Vec<Example>> = RefCell::new(Vec::new());
    static RUNS: Cell<u32> = Cell::new(0);
    static COMPILING: Cell<bool> = Cell::new(false);
}

// ── The crate being edited ──────────────────────────────────────────────

fn root() -> String {
    ROOT.with_borrow(|root| root.clone())
}

fn current() -> String {
    CURRENT.with_borrow(|current| current.clone())
}

fn new_state(text: &str) -> &'static EditorState {
    create_state(&StateConfig { doc: text.to_string(), extensions: SOURCE_EXTENSIONS.get() })
}

/// `files.set(path, state)`: in its place if it's there, else at the end.
fn set_file(path: &str, state: &'static EditorState) {
    FILES.with_borrow_mut(|files| match files.iter_mut().find(|(p, _)| p == path) {
        Some(file) => file.1 = state,
        None => files.push((path.to_string(), state)),
    });
}

fn has_file(path: &str) -> bool {
    FILES.with_borrow(|files| files.iter().any(|(p, _)| p == path))
}

fn file_state(path: &str) -> Option<&'static EditorState> {
    FILES.with_borrow(|files| match files.iter().find(|(p, _)| p == path) {
        Some((_, state)) => Some(*state),
        None => None,
    })
}

fn open_file(path: &str) {
    let open = current();
    if !open.is_empty() && has_file(&open) {
        set_file(&open, editor_state(SOURCE.get()));
    }
    CURRENT.set(path.to_string());
    set_editor_state(SOURCE.get(), file_state(path).expect("an open file"));
    // A stored state has the theme from when it was created: bring it up to date.
    dispatch(SOURCE.get(), &effect(reconfigure(SOURCE_THEME.get(), theme_for(is_dark()))));
    render_source_files();
}

fn render_source_files() {
    let paths = FILES.with_borrow(|files| files.iter().map(|(p, _)| p.clone()).collect());
    render_tree(
        by_id("source-files"),
        paths,
        TreeOptions {
            selected: current(),
            first: Some(root()),
            on_open: Rc::new(|path| open_file(&path)),
            decorate: Some(Rc::new(|li, path| {
                if path == root() {
                    let note = document::create_element(document, "span");
                    element::set_class_name(note, "text-[11px] text-muted");
                    node::set_text_content(note, "root ");
                    element::append(li, note);
                    return;
                }
                let remove = document::create_element(document, "button");
                element::set_class_name(remove, "invisible cursor-pointer px-1.5 text-muted group-hover:visible focus:visible");
                node::set_text_content(remove, "×");
                element::set_attribute(remove, "aria-label", &format!("Delete {path}"));
                let deleted = path.clone();
                event_target::add_event_listener(remove, "click", Box::new(move |_| {
                    if !window::confirm_with_message(window, &format!("Delete {deleted}?")) {
                        return;
                    }
                    FILES.with_borrow_mut(|files| files.retain(|(p, _)| *p != deleted));
                    if current() == deleted {
                        CURRENT.set(String::new());
                        open_file(&root());
                    } else {
                        render_source_files();
                    }
                }));
                element::append(li, remove);
            })),
        },
    );
}

fn create_file() {
    let answer = match window::prompt_with_message(window, "New file, e.g. math.rs or geometry/shape.rs:") {
        Some(answer) => answer,
        None => return,
    };
    let path = answer.trim().to_string();
    if path.is_empty() {
        return;
    }
    let module_path = reg_exp::new(r"^([a-z_][a-z0-9_]*/)*[a-z_][a-z0-9_]*\.rs$", "");
    if !reg_exp::test(module_path, &path) {
        set_status(&format!("\"{path}\" isn't a Rust module file name, like math.rs or geometry/shape.rs."), "bad");
        return;
    }
    if has_file(&path) {
        set_status(&format!("{path} already exists."), "bad");
        return;
    }
    set_file(&path, new_state(""));
    open_file(&path);
    let file = match path.rsplit_once('/') {
        Some((_, file)) => file,
        None => path.as_str(),
    };
    let module = file.strip_suffix(".rs").unwrap_or(file);
    set_status(&format!("Created {path}. Declare it with `mod {module};` in its parent, or rustc won't include it."), "");
}

/// The crate's files as text, including unsaved edits in the open file.
fn crate_sources() -> &'static JsMap {
    set_file(&current(), editor_state(SOURCE.get()));
    let texts = FILES.with_borrow(|files| files.iter().map(|(path, state)| (path.clone(), text_string(doc(state)))).collect());
    new_text_map(texts)
}

// ── Generated JS ────────────────────────────────────────────────────────

fn root_js() -> String {
    let root = root();
    match root.strip_suffix(".rs") {
        Some(stem) => format!("{stem}.js"),
        None => root.clone(),
    }
}

fn output_text(path: &str) -> Option<String> {
    OUTPUTS.with_borrow(|outputs| match outputs.iter().find(|(p, _)| p == path) {
        Some((_, text)) => Some(text.clone()),
        None => None,
    })
}

fn open_output(path: &str) {
    SHOWN_OUTPUT.set(path.to_string());
    let text = output_text(path).expect("a file the compile wrote");
    replace_text(OUTPUT.get(), &text, Some(reconfigure(OUTPUT_LANGUAGE.get(), javascript_language())));
    render_output_files();
}

fn render_output_files() {
    let list = by_id("output-files");
    let paths: Vec<String> = OUTPUTS.with_borrow(|outputs| outputs.iter().map(|(p, _)| p.clone()).collect());
    if paths.is_empty() {
        let empty = document::create_element(document, "li");
        element::set_class_name(empty, "flex items-center px-2 py-0.5 text-muted");
        node::set_text_content(empty, "(none)");
        element::replace_children(list, empty);
        return;
    }
    let shown = SHOWN_OUTPUT.with_borrow(|shown| shown.clone());
    render_tree(list, paths, TreeOptions { selected: shown, first: Some(root_js()), on_open: Rc::new(|path| open_output(&path)), decorate: None });
}

fn show_diagnostics(text: &str) {
    OUTPUTS.set(Vec::new());
    SHOWN_OUTPUT.set(String::new());
    replace_text(OUTPUT.get(), text, Some(reconfigure(OUTPUT_LANGUAGE.get(), together(Vec::new()))));
    render_output_files();
}

// ── Loading ─────────────────────────────────────────────────────────────

async fn fetch_example_file(name: String, path: String) -> (String, String) {
    let response = window::fetch_with_str(window, &format!("./examples/{name}/{path}")).await;
    (path, response::text(response).await)
}

async fn load_example(name: String, root: String, paths: Vec<String>) {
    // All at once: each download starts as it's made (ADR 0029).
    let mut downloads = Vec::new();
    for path in paths {
        downloads.push(fetch_example_file(name.clone(), path));
    }
    let mut texts = Vec::new();
    for download in downloads {
        texts.push(download.await);
    }
    FILES.with_borrow_mut(|files| files.clear());
    for (path, text) in texts {
        set_file(&path, new_state(&text));
    }
    ROOT.set(root);
    CURRENT.set(String::new());
    open_file(&self::root());
    OUTPUTS.set(Vec::new());
    SHOWN_OUTPUT.set(String::new());
    replace_text(OUTPUT.get(), "", None);
    render_output_files();
    run_program(new_text_map(Vec::new()), &root_js(), false);
}

/// The example called `name`: its name, root and files.
fn example_named(name: &str) -> Option<(String, String, Vec<String>)> {
    EXAMPLES.with_borrow(|examples| match examples.iter().find(|e| e.name == name) {
        Some(e) => Some((e.name.clone(), e.root.clone(), e.files.clone())),
        None => None,
    })
}

/// Start the page: listen, load, and show the first example.
/// An element that isn't on the page yet.
fn detached() -> &'static Element {
    document::create_element(document, "div")
}

pub async fn start() {
    // React renders the page, at once, so the code below finds its parts.
    let root = create_root(by_id("app"));
    flush_sync(move || root.render(component(page::App, ())));
    node::append_child(by_id("source"), editor_dom(SOURCE.get()));
    node::append_child(by_id("output"), editor_dom(OUTPUT.get()));
    listen_for_reports();
    event_target::add_event_listener(DARK_MODE.get(), "change", Box::new(|e| {
        let dark = media_query_list_event::matches(media_query_list_event::unchecked_from(e));
        dispatch(SOURCE.get(), &effect(reconfigure(SOURCE_THEME.get(), theme_for(dark))));
        dispatch(OUTPUT.get(), &effect(reconfigure(OUTPUT_THEME.get(), theme_for(dark))));
    }));
    event_target::add_event_listener(by_id("new-file"), "click", Box::new(|_| create_file()));

    let loaded = load().await;
    MODULE.set(Some(loaded.module));
    SYSROOT.set(Some(loaded.sysroot));
    WEB_CRATE.set(Some(loaded.web_crate));
    let select: &HtmlSelectElement = html_select_element::unchecked_from(by_id("example"));
    for example in &loaded.examples {
        let option = html_option_element::unchecked_from(document::create_element(document, "option"));
        html_option_element::set_text(option, &example.title);
        html_option_element::set_value(option, &example.name);
        html_select_element::add(select, option);
    }
    let first = match loaded.examples.first() {
        Some(e) => (e.name.clone(), e.root.clone(), e.files.clone()),
        None => return,
    };
    EXAMPLES.set(loaded.examples);
    event_target::add_event_listener(select, "change", Box::new(move |_| {
        let chosen = html_select_element::value(select);
        spawn(Box::new(async move {
            if let Some((name, root, files)) = example_named(&chosen) {
                load_example(name, root, files).await;
            }
            set_status("Ready.", "");
        }));
    }));
    let (name, root, files) = first;
    load_example(name, root, files).await;
    let (compile_button, test_button) = (button_by_id("compile"), button_by_id("test"));
    event_target::add_event_listener(compile_button, "click", Box::new(|_| spawn(Box::new(on_compile(false)))));
    event_target::add_event_listener(test_button, "click", Box::new(|_| spawn(Box::new(on_compile(true)))));
    html_button_element::set_disabled(compile_button, false);
    html_button_element::set_disabled(test_button, false);
    set_status("Ready.", "");
}

async fn on_compile(test: bool) {
    let compile_button = button_by_id("compile");
    let test_button = button_by_id("test");
    if COMPILING.get() || html_button_element::disabled(compile_button) {
        return;
    }
    COMPILING.set(true);
    html_button_element::set_disabled(compile_button, true);
    html_button_element::set_disabled(test_button, true);
    set_status(if test { "Compiling the tests…" } else { "Compiling…" }, "");
    let (module, sysroot, web_crate) = match (MODULE.get(), SYSROOT.get(), WEB_CRATE.get()) {
        (Some(module), Some(sysroot), Some(web_crate)) => (module, sysroot, web_crate),
        _ => unreachable!("the buttons are enabled once everything's loaded"),
    };
    let r = compile(module, sysroot, web_crate, crate_sources(), &root(), test).await;
    RUNS.set(RUNS.get() + 1);
    if r.ok {
        OUTPUTS.set(text_entries(r.files));
        // Keep showing the same file if it's still there; otherwise the root's.
        let shown = SHOWN_OUTPUT.with_borrow(|shown| shown.clone());
        let show = if output_text(&shown).is_some() { shown } else { root_js() };
        open_output(&show);
        let count = OUTPUTS.with_borrow(|outputs| outputs.len());
        set_status(&format!("Compiled: {count} JS file{}.", if count == 1 { "" } else { "s" }), "good");
        run_program(r.files, &root_js(), test);
    } else {
        show_diagnostics(&r.stderr);
        run_program(new_text_map(Vec::new()), &root_js(), false);
        set_status(&format!("Failed: exit {}.", r.exit), "bad");
    }
    let result = if r.ok { "ok" } else { "error" };
    let times = format!("instantiate {}, run {}, memory {}, {result}", ms(r.instantiate), ms(r.run), mb(r.memory as f64));
    stat(&format!("compile #{}", RUNS.get()), &times);
    COMPILING.set(false);
    html_button_element::set_disabled(compile_button, false);
    html_button_element::set_disabled(test_button, false);
    // For automated checks.
    set_last_result(window, &r);
}
