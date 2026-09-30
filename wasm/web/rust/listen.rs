// Listening to the DOM from an effect, whose cleanup stops it.

use webapi::{AbortController, AddEventListenerOptions, Event, EventTarget, abort_controller, event_target};

/// `target.addEventListener(type, f, { signal })`: until `controller` aborts.
pub fn listen(target: &EventTarget, type_: &str, f: Box<dyn FnMut(&Event)>, controller: &AbortController) {
    event_target::add_event_listener_with_options(
        target,
        type_,
        f,
        AddEventListenerOptions {
            signal: Some(abort_controller::signal(controller)),
            ..Default::default()
        },
    );
}
