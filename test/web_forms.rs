//! Every JS form the web crate's bindings use (ADR 0024), for the test to
//! check in the generated JS.

use web::{Event, document, element, event, event_target, html_input_element, node, text_decoder, text_encoder, uint8_array, window};

pub fn forms() -> String {
    let input = html_input_element::unchecked_from(document::create_element(document, "input"));
    html_input_element::set_value(input, "typed");
    let app = document::get_element_by_id(document, "app").expect("the page has an #app");
    element::append(app, input);
    element::append_with_str(app, "!");
    let ping: &Event = event::new("ping");
    event_target::add_event_listener(app, "ping", Box::new(|e| event::prevent_default(e)));
    let _ = event_target::dispatch_event(window, ping);
    node::text_content(app).unwrap() + &html_input_element::value(input)
}

/// Optional arguments give more forms: `encode_with_input(e, text)` is
/// `e.encode(text)`, and a union's members are named by type:
/// `decode_with_uint8_array(d, bytes)` is `d.decode(bytes)`.
pub fn round_trip(text: &str) -> (u32, String) {
    let bytes = text_encoder::encode_with_input(text_encoder::new(), text);
    let back = text_decoder::decode_with_uint8_array(text_decoder::new_with_label("utf-8"), bytes);
    (uint8_array::length(bytes), back)
}
