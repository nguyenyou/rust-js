// A counter, written against the DOM through the `web` crate: bindings
// generated from W3C's WebIDL (ADR 0024). Also closures (ADR 0022), and
// strings, references and shared state (ADR 0023).

use std::cell::Cell;
use std::rc::Rc;

// Each DOM interface is a type and a module of its members:
// `document::create_element(document, "p")` is `document.createElement("p")`.
use web::{Element, document, element, event_target, node};

fn button(label: &str) -> &'static Element {
    let b = document::create_element(document, "button");
    node::set_text_content(b, label);
    b
}

/// A button that adds `by` to the shared count, and shows the new count.
fn stepper(label: &str, by: i32, count: &Rc<Cell<i32>>, output: &'static Element) -> &'static Element {
    let b = button(label);
    let count = count.clone();
    event_target::add_event_listener(b, "click", Box::new(move |_| {
        count.set(count.get() + by);
        node::set_text_content(output, &count.get().to_string());
    }));
    b
}

pub fn main() {
    let app = document::get_element_by_id(document, "app");
    // Both buttons change one count, so they share it: `Rc` to share, `Cell`
    // to change it through a shared reference.
    let count = Rc::new(Cell::new(0));
    let output = document::create_element(document, "output");
    node::set_text_content(output, "0");
    // An `Element` is a `Node` (`Deref`), so it goes where `append` wants a `Node`.
    element::append(app, stepper("-", -1, &count, output));
    element::append(app, output);
    element::append(app, stepper("+", 1, &count, output));
}
