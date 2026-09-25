// A counter, written against the DOM directly: JS interop (ADR 0021),
// closures (ADR 0022), and strings, references and shared state (ADR 0023).
#![feature(extern_types)]

use std::cell::Cell;
use std::rc::Rc;

// What we use from JS. `type` declares an opaque JS value, a `static` a
// global, and a `fn` a function. A first parameter named `this` makes a
// method call: `create_element(document, "p")` is `document.createElement("p")`.
unsafe extern "Rust" {
    type Document;
    type Element;

    safe static document: &'static Document;

    #[link_name = "getElementById"]
    safe fn get_element_by_id(this: &Document, id: &str) -> &'static Element;
    #[link_name = "createElement"]
    safe fn create_element(this: &Document, tag: &str) -> &'static Element;
    safe fn append(this: &Element, child: &Element);
    /// Replaces the element's contents with `text`.
    #[link_name = "replaceChildren"]
    safe fn set_text(this: &Element, text: &str);
    #[link_name = "addEventListener"]
    safe fn add_event_listener(this: &Element, event: &str, listener: Box<dyn FnMut()>);
}

fn button(label: &str) -> &'static Element {
    let b = create_element(document, "button");
    set_text(b, label);
    b
}

/// A button that adds `by` to the shared count, and shows the new count.
fn stepper(label: &str, by: i32, count: &Rc<Cell<i32>>, output: &'static Element) -> &'static Element {
    let b = button(label);
    let count = count.clone();
    add_event_listener(b, "click", Box::new(move || {
        count.set(count.get() + by);
        set_text(output, &count.get().to_string());
    }));
    b
}

pub fn main() {
    let app = get_element_by_id(document, "app");
    // Both buttons change one count, so they share it: `Rc` to share, `Cell`
    // to change it through a shared reference.
    let count = Rc::new(Cell::new(0));
    let output = create_element(document, "output");
    set_text(output, "0");
    append(app, stepper("-", -1, &count, output));
    append(app, output);
    append(app, stepper("+", 1, &count, output));
}
