// A counter in React (ADR 0041): a component with state, written with
// `jsx!`, which rust-js prints as the JSX you'd write by hand, and a `main`
// that renders it into the page's `#app`.

#![allow(non_snake_case)]

use react::dom::client::create_root;
use react::{Element, jsx, use_state};
use webapi::document;

pub fn Counter() -> Element {
    let (count, set_count) = use_state(0);
    jsx! {
        <div className="counter">
            <button onClick={move |_| set_count.update(|n| n - 1)}>{"−"}</button>
            <output>{count}</output>
            <button onClick={move |_| set_count.update(|n| n + 1)}>{"+"}</button>
        </div>
    }
}

pub fn main() {
    let app = document::get_element_by_id(document, "app").expect("the page has an #app");
    create_root(app).render(jsx! { <Counter /> });
}
