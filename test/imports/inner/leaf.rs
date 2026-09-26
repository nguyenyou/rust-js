// Two directories down, `./greet.js` still means the file beside the root's JS.
unsafe extern "Rust" {
    #[link_name = "./greet.js#default"]
    safe fn greet(name: &str) -> String;
}

pub fn hello() -> String {
    greet("leaf")
}
