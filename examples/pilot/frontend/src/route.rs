//! Where the app is: its URL's hash, `#/contacts/2`, followed as it changes.

use react::{use_effect, use_state};
use webapi::{AddEventListenerOptions, abort_controller, event_target, location, window};

#[derive(Clone, Copy, PartialEq)]
pub enum Route {
    List,
    Contact(u32),
    New,
    NotFound,
}

pub fn parse(hash: &str) -> Route {
    match hash.strip_prefix('#').unwrap_or(hash) {
        "" | "/" => Route::List,
        "/new" => Route::New,
        path => match path.strip_prefix("/contacts/").and_then(|id| id.parse().ok()) {
            Some(id) => Route::Contact(id),
            None => Route::NotFound,
        },
    }
}

fn hash() -> String {
    location::hash(window::location(window))
}

/// Go to `hash`, as a link would.
pub fn go(to: &str) {
    location::set_hash(window::location(window), to);
}

/// The route, and a render each time it changes.
pub fn use_route() -> Route {
    let (current, set_current) = use_state(hash());
    use_effect(
        move || {
            let controller = abort_controller::new();
            // It's removed when the effect is cleaned up: its signal aborts.
            let options = AddEventListenerOptions {
                signal: Some(abort_controller::signal(controller)),
                ..Default::default()
            };
            event_target::add_event_listener_with_options(
                window,
                "hashchange",
                Box::new(move |_| set_current.set(hash())),
                options,
            );
            move || abort_controller::abort(controller)
        },
        (),
    );
    parse(current)
}
