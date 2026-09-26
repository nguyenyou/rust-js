// The page, rendered by React (ADR 0044). lib.rs still fills in its
// parts by their ids: the editors, the file trees, the toolbar's state and
// the Result frame. They move into components a part at a time.
//
// It's styled with Tailwind (ADR 0045); the colors are styles.css's theme.

use react::html::{button, code, div, h1, h2, iframe, kbd, nav, p, section, select, span, table, ul};
use react::{Element, fragment};

/// The example picker and the buttons.
const CONTROL: &str = "rounded-md border border-line bg-panel px-3 py-1 enabled:cursor-pointer enabled:hover:bg-selected disabled:opacity-50";
const HEADING: &str = "mb-1.5 text-[13px] font-semibold text-muted";
/// One side of the page: a file explorer beside an editor.
const PANE: &str = "grid h-[460px] grid-cols-[150px_minmax(0,1fr)] overflow-hidden rounded-md border border-line";
const EXPLORER: &str = "overflow-auto border-r border-line bg-panel py-1.5 font-mono text-[13px]";
/// CodeMirror makes the `.cm-editor` inside.
const EDITOR: &str = "min-w-0 overflow-hidden [&_.cm-editor]:h-full [&_.cm-editor]:text-[13px]";

pub fn App() -> Element {
    fragment((
        h1().class_name("mb-1 text-lg font-bold").children("rust-js playground"),
        p().class_name("mb-3 text-muted").children("rustc's front end and rust-js, as WebAssembly. No server compiles anything."),
        div().class_name("mb-3 flex flex-wrap items-center gap-x-3 gap-y-2").children((
            select().id("example").class_name(CONTROL).attr("aria-label", "Example"),
            button().id("compile").class_name(CONTROL).disabled(true).children("Compile"),
            button()
                .id("test")
                .class_name(CONTROL)
                .disabled(true)
                .title("Compile with --test and run the #[test] functions")
                .children("Test"),
            kbd().class_name("font-mono text-xs text-muted").children("⌘/Ctrl-Enter"),
            span().id("status").role("status").children("Loading…"),
        )),
        div().class_name("grid grid-cols-[repeat(auto-fit,minmax(min(100%,440px),1fr))] gap-3").children((
            section().children((
                h2().class_name(HEADING).children("Rust"),
                div().class_name(PANE).children((
                    nav().class_name(EXPLORER).attr("aria-label", "Rust files").children((
                        ul().id("source-files"),
                        button().id("new-file").class_name("mx-2 mt-1.5 block cursor-pointer text-muted").children("+ New file"),
                    )),
                    div().id("source").class_name(EDITOR),
                )),
            )),
            section().children((
                h2().class_name(HEADING).children("JavaScript"),
                div().class_name(PANE).children((
                    nav().class_name(EXPLORER).attr("aria-label", "JavaScript files").children(ul().id("output-files")),
                    div().id("output").class_name(EDITOR),
                )),
            )),
        )),
        section().id("result-section").class_name("mt-3").hidden(true).children((
            h2().class_name(HEADING).children((
                "Result ",
                span().class_name("font-normal").children((
                    "the root module's ",
                    code().children("main()"),
                    ", or with Test its ",
                    code().children("#[test]"),
                    "s, in a frame of their own",
                )),
            )),
            iframe().id("result").class_name("block h-[280px] w-full rounded-md border border-line bg-page").title("Result"),
        )),
        table().id("stats").class_name("mt-3"),
    ))
}
