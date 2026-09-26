//! Imports from JS modules: `#[link_name = "module#path"]` (ADR 0028).
#![feature(extern_types)]

pub mod inner {
    pub mod leaf;
}

unsafe extern "Rust" {
    type Url;

    // Named imports, and a path inside one.
    #[link_name = "node:path#join"]
    safe fn path_join(a: &str, b: &str) -> String;
    #[link_name = "node:path#posix.basename"]
    safe fn basename(path: &str) -> String;

    // A class from a module, next to the global of the same name.
    #[link_name = "new node:url#URL"]
    safe fn new_url(href: &str) -> &'static Url;
    #[link_name = "new URL"]
    safe fn new_global_url(href: &str) -> &'static Url;
    #[link_name = "get href"]
    safe fn href(this: &Url) -> String;

    // A file of our own, beside the root's JS: its default export, and a
    // path through the whole module.
    #[link_name = "./greet.js#default"]
    safe fn greet(name: &str) -> String;
    #[link_name = "./greet.js#*.polite"]
    safe fn polite(name: &str) -> String;
    #[link_name = "./greet.js#punctuation"]
    safe static punctuation: &'static str;
}

pub fn paths() -> String {
    // A local named like an import doesn't hide it.
    let join = "c.txt";
    path_join(&path_join("a", "b"), join)
}

pub fn file_name() -> String {
    basename("/tmp/x/y.txt")
}

pub fn urls() -> (String, String) {
    (href(new_url("https://example.com/a")), href(new_global_url("https://example.com/b")))
}

pub fn greetings() -> (String, String, &'static str) {
    (greet("world"), polite("world"), punctuation)
}
