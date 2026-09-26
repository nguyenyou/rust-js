// Listening to the DOM from an effect, whose cleanup stops it.

use web::{AbortController, AbortSignal, Event, EventTarget, abort_controller};

unsafe extern "Rust" {
    #[link_name = "addEventListener"]
    safe fn add_event_listener_until(this: &EventTarget, type_: &str, callback: Box<dyn FnMut(&Event)>, options: &Until);
}

#[allow(dead_code)]
struct Until {
    signal: &'static AbortSignal,
}

/// `target.addEventListener(type, f, { signal })`: until `controller` aborts.
pub fn listen(target: &EventTarget, type_: &str, f: Box<dyn FnMut(&Event)>, controller: &AbortController) {
    add_event_listener_until(target, type_, f, &Until { signal: abort_controller::signal(controller) });
}
