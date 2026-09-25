//! Every JS form the web crate's bindings use (ADR 0024), for the test to
//! check in the generated JS.

use web::{Event, document, element, event, event_target, html_input_element, node, window};

pub fn forms() -> String {
    let input = html_input_element::unchecked_from(document::create_element(document, "input"));
    html_input_element::set_value(input, "typed");
    let app = document::get_element_by_id(document, "app");
    element::append(app, input);
    element::append_with_str(app, "!");
    let ping: &Event = event::new("ping");
    event_target::add_event_listener(app, "ping", Box::new(|e| event::prevent_default(e)));
    let _ = event_target::dispatch_event(window, ping);
    node::text_content(app) + &html_input_element::value(input)
}
