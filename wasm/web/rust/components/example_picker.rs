// Which example is in the editor.

use std::rc::Rc;

use react::Element;
use react::event::Change;
use react::html::{option, select};

use crate::compiler::Example;
use crate::styles::CONTROL;

pub struct ExamplePickerProps {
    pub examples: &'static [Example],
    pub chosen: String,
    pub on_choose: Rc<dyn Fn(String)>,
}

pub fn ExamplePicker(ExamplePickerProps { examples, chosen, on_choose }: ExamplePickerProps) -> Element {
    select()
        .id("example")
        .class_name(CONTROL)
        .attr("aria-label", "Example")
        .value(chosen)
        .on_change(move |e: &Change| on_choose(e.value()))
        .children(
            examples
                .iter()
                .map(|example| option().key(example.name.clone()).value(example.name.clone()).children(example.title.clone()))
                .collect::<Vec<_>>(),
        )
}
