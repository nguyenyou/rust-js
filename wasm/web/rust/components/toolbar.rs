// Above the editors: the example, Compile and Test, and the status line.

use std::rc::Rc;

use react::Element;

use super::example_picker::ExamplePicker;
use super::status_line::{Status, StatusLine};
use crate::compiler::Example;
use crate::styles::CONTROL;

pub struct ToolbarProps {
    pub examples: &'static [Example],
    pub example: String,
    pub on_example: Rc<dyn Fn(String)>,
    /// Whether Compile and Test can run: loaded, and not compiling already.
    pub ready: bool,
    /// Compile, or with `true`, compile the tests and run them.
    pub on_compile: Rc<dyn Fn(bool)>,
    pub status: &'static Status,
}

pub fn Toolbar(ToolbarProps { examples, example, on_example, ready, on_compile, status }: ToolbarProps) -> Element {
    let on_test = on_compile.clone();
    jsx! {
        <div className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-2">
            <ExamplePicker examples={examples} chosen={example} onChoose={on_example} />
            <button id="compile" className={CONTROL} disabled={!ready} onClick={move |_| on_compile(false)}>
                {"Compile"}
            </button>
            <button
                id="test"
                className={CONTROL}
                disabled={!ready}
                title="Compile with --test and run the #[test] functions"
                onClick={move |_| on_test(true)}
            >
                {"Test"}
            </button>
            <kbd className="font-mono text-xs text-muted">{"⌘/Ctrl-Enter"}</kbd>
            <StatusLine status={status} />
        </div>
    }
}
