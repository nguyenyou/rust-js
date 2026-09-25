// A todo list, written against the DOM through the `web` crate (ADR 0024).
// The todos live in one `Rc<RefCell<State>>`; after every change, the list
// is drawn again from it (ADR 0025).

use std::cell::RefCell;
use std::rc::Rc;

use web::{
    Element, document, element, event_target, html_element, html_input_element, keyboard_event, node,
    css_style_declaration,
};

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Active,
    Completed,
}

struct Todo {
    id: u32,
    title: String,
    done: bool,
}

struct State {
    todos: Vec<Todo>,
    next_id: u32,
    filter: Filter,
}

type Shared = Rc<RefCell<State>>;

/// The parts of the page that change.
#[derive(Clone, Copy)]
struct View {
    list: &'static Element,
    left: &'static Element,
}

fn create(tag: &str) -> &'static Element {
    document::create_element(document, tag)
}

fn text(tag: &str, s: &str) -> &'static Element {
    let e = create(tag);
    node::set_text_content(e, s);
    e
}

/// When `event` fires on `target`, apply `change` to the state and draw it again.
fn on(target: &'static Element, event: &str, state: &Shared, view: View, change: Box<dyn Fn(&mut State)>) {
    let state = state.clone();
    event_target::add_event_listener(target, event, Box::new(move |_| {
        change(&mut state.borrow_mut());
        render(&state, view);
    }));
}

fn add(s: &mut State, title: &str) {
    s.todos.push(Todo { id: s.next_id, title: title.to_string(), done: false });
    s.next_id += 1;
}

fn toggle(s: &mut State, id: u32) {
    for todo in &mut s.todos {
        if todo.id == id {
            todo.done = !todo.done;
        }
    }
}

fn shown(filter: Filter, todo: &Todo) -> bool {
    match filter {
        Filter::All => true,
        Filter::Active => !todo.done,
        Filter::Completed => todo.done,
    }
}

/// One `<li>`: a checkbox, the title, and a button to delete it.
fn item(state: &Shared, view: View, todo: &Todo) -> &'static Element {
    let li = create("li");
    let id = todo.id;

    let check = html_input_element::unchecked_from(create("input"));
    html_input_element::set_type(check, "checkbox");
    html_input_element::set_checked(check, todo.done);
    on(check, "change", state, view, Box::new(move |s| toggle(s, id)));

    let title = html_element::unchecked_from(text("span", &todo.title));
    if todo.done {
        css_style_declaration::set_property(html_element::style(title), "text-decoration", "line-through");
    }

    let delete = text("button", "×");
    on(delete, "click", state, view, Box::new(move |s| s.todos.retain(|t| t.id != id)));

    element::append(li, check);
    element::append(li, title);
    element::append(li, delete);
    li
}

fn render(state: &Shared, view: View) {
    let s = state.borrow();
    node::set_text_content(view.list, "");
    let mut left = 0;
    for todo in &s.todos {
        if !todo.done {
            left += 1;
        }
        if shown(s.filter, todo) {
            element::append(view.list, item(state, view, todo));
        }
    }
    let noun = if left == 1 { " item left" } else { " items left" };
    node::set_text_content(view.left, &(left.to_string() + noun));
}

pub fn main() {
    let app = document::get_element_by_id(document, "app");
    let state: Shared = Rc::new(RefCell::new(State { todos: Vec::new(), next_id: 1, filter: Filter::All }));
    let view = View { list: create("ul"), left: create("span") };

    // Typing a title and pressing Enter adds it.
    let input = html_input_element::unchecked_from(create("input"));
    html_input_element::set_placeholder(input, "What needs to be done?");
    let adding = state.clone();
    event_target::add_event_listener(input, "keydown", Box::new(move |e| {
        let title = html_input_element::value(input);
        let title = title.trim();
        if keyboard_event::key(keyboard_event::unchecked_from(e)) == "Enter" && !title.is_empty() {
            add(&mut adding.borrow_mut(), title);
            html_input_element::set_value(input, "");
            render(&adding, view);
        }
    }));

    // Which todos to show, and clearing the completed ones.
    let footer = create("p");
    element::append(footer, view.left);
    for (label, filter) in [("All", Filter::All), ("Active", Filter::Active), ("Completed", Filter::Completed)] {
        let b = text("button", label);
        on(b, "click", &state, view, Box::new(move |s| s.filter = filter));
        element::append(footer, b);
    }
    let clear = text("button", "Clear completed");
    on(clear, "click", &state, view, Box::new(|s| s.todos.retain(|t| !t.done)));
    element::append(footer, clear);

    element::append(app, input);
    element::append(app, view.list);
    element::append(app, footer);
    render(&state, view);
}

// Tests, in Rust (ADR 0026): `rust-js --test` compiles them, and `bun test`
// runs them in happy-dom's DOM.
#[cfg(test)]
mod tests {
    use super::*;
    use web::{Event, HtmlInputElement, node_list};

    /// `new KeyboardEvent(type, { key })`. The web crate can't take
    /// dictionaries yet, but a struct is a JS object with the same fields
    /// (ADR 0020), so it can be declared here (ADR 0021).
    #[allow(dead_code)] // `key` is read by JS, not by Rust.
    struct KeyboardEventInit {
        key: String,
    }

    unsafe extern "Rust" {
        #[link_name = "new KeyboardEvent"]
        safe fn keyboard_event(type_: &str, init: &KeyboardEventInit) -> &'static Event;
    }

    /// An empty page with the `<div id="app">` that `main` looks for.
    fn page() -> &'static Element {
        let body = document::body(document);
        node::set_text_content(body, "");
        let app = create("div");
        element::set_id(app, "app");
        element::append(body, app);
        main();
        app
    }

    fn input(app: &Element) -> &'static HtmlInputElement {
        html_input_element::unchecked_from(element::query_selector(app, "input"))
    }

    /// Type `title` and press `key`.
    fn type_in(app: &Element, title: &str, key: &str) {
        let field = input(app);
        html_input_element::set_value(field, title);
        event_target::dispatch_event(field, keyboard_event("keydown", &KeyboardEventInit { key: key.to_string() }));
    }

    fn titles(app: &Element) -> Vec<String> {
        let spans = element::query_selector_all(app, "li span");
        let mut titles = Vec::new();
        for i in 0..node_list::length(spans) {
            titles.push(node::text_content(node_list::item(spans, i)));
        }
        titles
    }

    fn left(app: &Element) -> String {
        node::text_content(element::query_selector(app, "p span"))
    }

    /// The `n`th element matching `selector`, to click.
    fn nth(app: &Element, selector: &str, n: u32) -> &'static web::HtmlElement {
        html_element::unchecked_from(node_list::item(element::query_selector_all(app, selector), n))
    }

    #[test]
    fn starts_empty() {
        let app = page();
        assert_eq!(titles(app), Vec::<String>::new());
        assert_eq!(left(app), "0 items left");
    }

    #[test]
    fn enter_adds_a_trimmed_title() {
        let app = page();
        type_in(app, "  Buy milk  ", "Enter");
        assert_eq!(titles(app), ["Buy milk"]);
        assert_eq!(html_input_element::value(input(app)), "");
        assert_eq!(left(app), "1 item left");
    }

    #[test]
    fn blank_titles_and_other_keys_add_nothing() {
        let app = page();
        type_in(app, "   ", "Enter");
        type_in(app, "Not yet", "a");
        assert_eq!(titles(app), Vec::<String>::new());
    }

    #[test]
    fn ticking_one_off_crosses_it_out() {
        let app = page();
        type_in(app, "Buy milk", "Enter");
        type_in(app, "Walk the dog", "Enter");
        html_element::click(nth(app, "li input", 0));
        assert_eq!(left(app), "1 item left");
        let title = nth(app, "li span", 0);
        assert_eq!(css_style_declaration::get_property_value(html_element::style(title), "text-decoration"), "line-through");
    }

    #[test]
    fn filters_clear_and_delete() {
        let app = page();
        type_in(app, "Buy milk", "Enter");
        type_in(app, "Walk the dog", "Enter");
        html_element::click(nth(app, "li input", 0));
        let (all, active, completed, clear) = (nth(app, "p button", 0), nth(app, "p button", 1), nth(app, "p button", 2), nth(app, "p button", 3));
        html_element::click(active);
        assert_eq!(titles(app), ["Walk the dog"]);
        html_element::click(completed);
        assert_eq!(titles(app), ["Buy milk"]);
        html_element::click(all);
        assert_eq!(titles(app), ["Buy milk", "Walk the dog"]);
        html_element::click(clear);
        assert_eq!(titles(app), ["Walk the dog"]);
        html_element::click(nth(app, "li button", 0));
        assert_eq!(titles(app), Vec::<String>::new());
        assert_eq!(left(app), "0 items left");
    }
}
