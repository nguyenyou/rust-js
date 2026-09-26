// A countdown, with async code (ADR 0029): an `async fn` that `.await`s a
// timer between steps, started from a click with `spawn`.

use std::cell::Cell;
use std::rc::Rc;

use web::{Element, Promise, document, element, event_target, node, spawn};

unsafe extern "Rust" {
    #[link_name = "setTimeout"]
    safe fn set_timeout(callback: Box<dyn FnOnce()>, ms: u32);
    #[link_name = "new Promise"]
    safe fn new_promise(executor: Box<dyn FnOnce(Box<dyn FnOnce()>)>) -> Promise<()>;
}

/// A promise that resolves after `ms` milliseconds.
fn sleep(ms: u32) -> Promise<()> {
    new_promise(Box::new(move |resolve| set_timeout(resolve, ms)))
}

async fn count_down(output: &'static Element, from: u32) {
    let mut n = from;
    while n > 0 {
        node::set_text_content(output, &n.to_string());
        sleep(500).await;
        n -= 1;
    }
    node::set_text_content(output, "Go!");
}

pub fn main() {
    let app = document::get_element_by_id(document, "app").expect("the page has an #app");
    let start = document::create_element(document, "button");
    node::set_text_content(start, "Start");
    let output = document::create_element(document, "output");
    // One countdown at a time.
    let running = Rc::new(Cell::new(false));
    event_target::add_event_listener(start, "click", Box::new(move |_| {
        if running.get() {
            return;
        }
        running.set(true);
        let running = running.clone();
        spawn(Box::new(async move {
            count_down(output, 3).await;
            running.set(false);
        }));
    }));
    element::append(app, start);
    element::append(app, output);
}

#[cfg(test)]
mod tests {
    use super::*;
    use web::html_element;

    /// An empty page with the `<div id="app">` that `main` looks for.
    fn page() -> &'static Element {
        let body = document::body(document).unwrap();
        node::set_text_content(body, "");
        let app = document::create_element(document, "div");
        element::set_id(app, "app");
        element::append(body, app);
        app
    }

    /// A promise runs up to its first `.await` as soon as it's made, so the
    /// click shows the first number before the handler returns.
    #[test]
    fn a_click_shows_the_first_number_at_once() {
        let app = page();
        main();
        html_element::click(html_element::unchecked_from(element::query_selector(app, "button").unwrap()));
        assert_eq!(node::text_content(element::query_selector(app, "output").unwrap()).unwrap(), "3");
    }
}
