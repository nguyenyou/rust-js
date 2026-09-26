// Which example is in the editor.

use std::rc::Rc;

use react::Element;
use react::event::Change;

use crate::compiler::Example;
use crate::styles::CONTROL;

pub struct ExamplePickerProps {
    pub examples: &'static [Example],
    pub chosen: String,
    pub on_choose: Rc<dyn Fn(String)>,
}

pub fn ExamplePicker(ExamplePickerProps { examples, chosen, on_choose }: ExamplePickerProps) -> Element {
    jsx! {
        <select
            id="example"
            className={CONTROL}
            aria-label="Example"
            value={chosen}
            onChange={move |e: &Change| on_choose(e.value())}
        >
            {examples.iter().map(|example| jsx! {
                <option key={example.name.clone()} value={example.name.clone()}>{example.title.clone()}</option>
            }).collect::<Vec<_>>()}
        </select>
    }
}
