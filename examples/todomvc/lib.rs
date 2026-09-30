//! [TodoMVC](https://todomvc.com), in Rust, with React: its markup, its
//! stylesheets, `todomvc-app-css` and `todomvc-common`'s, and every feature
//! its spec asks for. The todos are a reducer's (`model`), each one a
//! component (`item`), and the route, `#/active` say, the URL's hash.

#![allow(non_snake_case)]

js::import!("todomvc-common/base.css");
js::import!("todomvc-app-css/index.css");

mod item;
mod model;

use item::TodoItem;
use model::{Action, Filter, load, reduce, save};
use react::dom::client::create_root;
use react::event::{Change, Keyboard};
use react::{Element, use_effect, use_reducer_with, use_state};
use webapi::{AddEventListenerOptions, abort_controller, document, event_target, location, window};

/// The route's filter, and a render each time the hash changes.
fn use_filter() -> Filter {
    let hash = || location::hash(window::location(window));
    let (current, set_current) = use_state(hash());
    use_effect(
        move || {
            let controller = abort_controller::new();
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
    Filter::from_hash(current)
}

pub fn App() -> Element {
    let (todos, dispatch) = use_reducer_with(reduce, (), |_| load());
    let filter = use_filter();
    let (draft, set_draft) = use_state(String::new());
    use_effect(move || save(todos), (todos,));
    let active = todos.iter().filter(|todo| !todo.completed).count();
    let completed = todos.len() - active;
    let add = move |e: &Keyboard| {
        let title = draft.trim().to_string();
        if e.key() == "Enter" && !title.is_empty() {
            dispatch.dispatch(Action::Add(title));
            set_draft.set(String::new());
        }
    };
    let selected = move |route: Filter| if filter == route { "selected" } else { "" };
    jsx! {
        <>
            <section className="todoapp">
                <header className="header">
                    <h1>{"todos"}</h1>
                    <input
                        className="new-todo"
                        placeholder="What needs to be done?"
                        autoFocus={true}
                        value={draft.clone()}
                        onChange={move |e: &Change| set_draft.set(e.value())}
                        onKeyDown={add} />
                </header>
                {if todos.is_empty() { None } else { Some(jsx! {
                    <section className="main">
                        <input
                            id="toggle-all"
                            className="toggle-all"
                            type="checkbox"
                            checked={active == 0}
                            onChange={move |e: &Change| dispatch.dispatch(Action::ToggleAll(e.checked()))} />
                        <label htmlFor="toggle-all">{"Mark all as complete"}</label>
                        <ul className="todo-list">
                            {todos
                            .iter()
                            .filter(|todo| filter.shows(todo))
                            .map(|todo| jsx! { <TodoItem key={todo.id} todo={todo.clone()} dispatch={dispatch} /> })
                            .collect::<Vec<_>>()}
                        </ul>
                    </section>
                }) }}
                {if todos.is_empty() { None } else { Some(jsx! {
                    <footer className="footer">
                        <span className="todo-count">
                            <strong>{active}</strong>
                            {if active == 1 { " item left" } else { " items left" }}
                        </span>
                        <ul className="filters">
                            <li><a className={selected(Filter::All)} href="#/">{"All"}</a></li>
                            <li><a className={selected(Filter::Active)} href="#/active">{"Active"}</a></li>
                            <li><a className={selected(Filter::Completed)} href="#/completed">{"Completed"}</a></li>
                        </ul>
                        {if completed > 0 { Some(jsx! {
                            <button className="clear-completed" onClick={move |_| dispatch.dispatch(Action::ClearCompleted)}>
                                {"Clear completed"}
                            </button>
                        }) } else { None }}
                    </footer>
                }) }}
            </section>
            <footer className="info">
                <p>{"Double-click to edit a todo"}</p>
                <p>{"Written in Rust, compiled by "}<a href="https://github.com/rust-js-lang/rust-js">{"rust-js"}</a></p>
                <p>{"Part of "}<a href="http://todomvc.com">{"TodoMVC"}</a></p>
            </footer>
        </>
    }
}

pub fn main() {
    let app = document::get_element_by_id(document, "app").expect("the page has an #app");
    create_root(app).render(jsx! { <App /> });
}
