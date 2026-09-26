// The playground's own code, in Rust. When the site is built, rust-js
// compiles it to lib.js beside it, running as WebAssembly, the same compiler
// the page runs (see ../compile-rust.ts), and main.ts imports it. More of
// main.ts moves here, a part at a time.
//
// So far: loading what the page needs, the stats table, the file trees,
// running rust-js.wasm on a crate, under the WASI shim, and linking what it
// wrote into one script for the Result frame.

#![feature(extern_types)]

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use web::{
    Element, JsError, JsObject, Promise, RegExp, Response, Uint8Array, WebAssemblyInstance, WebAssemblyMemory,
    WebAssemblyModule, array_buffer, css_style_declaration, document, element, event_target, html_element,
    html_table_element, html_table_row_element, js_error, node, reg_exp, response, text_decoder, text_encoder, uint8_array,
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
    safe fn start(this: &Wasi, instance: &WebAssemblyInstance) -> Result<i32, &'static JsError>;
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
        let indent = format!("{}px", 8 + depth * 12);
        match entry {
            Entry::Folder(children) => {
                let label = html_element::unchecked_from(document::create_element(document, "span"));
                element::set_class_name(label, "folder");
                css_style_declaration::set_property(html_element::style(label), "padding-left", &indent);
                node::set_text_content(label, &format!("{name}/"));
                let nested = document::create_element(document, "ul");
                render(children, nested, depth + 1, options);
                let wrapper = html_element::unchecked_from(document::create_element(document, "div"));
                css_style_declaration::set_property(html_element::style(wrapper), "width", "100%");
                element::append(wrapper, label);
                element::append(wrapper, nested);
                element::append(li, wrapper);
            }
            Entry::File(path) => {
                let file = html_element::unchecked_from(document::create_element(document, "button"));
                element::set_class_name(file, "file");
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
    let started = start(wasi, instance);
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
