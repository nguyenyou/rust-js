// Above the editors: the example, Compile and Test, and the status line.

use std::rc::Rc;

use react::html::{button, div, kbd};
use react::{Element, component};

use super::example_picker::{ExamplePicker, ExamplePickerProps};
use super::status_line::{Status, StatusLine, StatusLineProps};
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
    div().class_name("mb-3 flex flex-wrap items-center gap-x-3 gap-y-2").children((
        component(ExamplePicker, ExamplePickerProps { examples, chosen: example, on_choose: on_example }),
        button().id("compile").class_name(CONTROL).disabled(!ready).on_click(move |_| on_compile(false)).children("Compile"),
        button()
            .id("test")
            .class_name(CONTROL)
            .disabled(!ready)
            .title("Compile with --test and run the #[test] functions")
            .on_click(move |_| on_test(true))
            .children("Test"),
        kbd().class_name("font-mono text-xs text-muted").children("⌘/Ctrl-Enter"),
        component(StatusLine, StatusLineProps { status }),
    ))
}
