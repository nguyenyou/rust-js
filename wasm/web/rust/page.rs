// The page, rendered by React (ADR 0044). lib.rs still fills in its
// parts by their ids: the editors, the file trees, the toolbar's state and
// the Result frame. They move into components a part at a time.

use react::html::{button, code, div, h1, h2, iframe, kbd, nav, p, section, select, span, table, ul};
use react::{Element, fragment};

pub fn App() -> Element {
    fragment((
        h1().children("rust-js playground"),
        p().children("rustc's front end and rust-js, as WebAssembly. No server compiles anything."),
        div().class_name("toolbar").children((
            select().id("example").attr("aria-label", "Example"),
            button().id("compile").disabled(true).children("Compile"),
            button()
                .id("test")
                .disabled(true)
                .title("Compile with --test and run the #[test] functions")
                .children("Test"),
            kbd().children("⌘/Ctrl-Enter"),
            span().id("status").role("status").children("Loading…"),
        )),
        div().class_name("panes").children((
            section().children((
                h2().children("Rust"),
                div().class_name("pane").children((
                    nav().class_name("explorer").attr("aria-label", "Rust files").children((
                        ul().id("source-files"),
                        button().class_name("new-file").id("new-file").children("+ New file"),
                    )),
                    div().class_name("editor").id("source"),
                )),
            )),
            section().children((
                h2().children("JavaScript"),
                div().class_name("pane").children((
                    nav().class_name("explorer").attr("aria-label", "JavaScript files").children(ul().id("output-files")),
                    div().class_name("editor").id("output"),
                )),
            )),
        )),
        section().id("result-section").hidden(true).children((
            h2().children((
                "Result ",
                span().class_name("hint").children((
                    "the root module's ",
                    code().children("main()"),
                    ", or with Test its ",
                    code().children("#[test]"),
                    "s, in a frame of their own",
                )),
            )),
            iframe().id("result").title("Result"),
        )),
        table().id("stats"),
    ))
}
