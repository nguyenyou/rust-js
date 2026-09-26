// React components (ADR 0041), compiled to JSX (ADR 0040) and rendered by
// React itself in test/fib.test.ts.

#![allow(non_snake_case)]

use react::event::{Change, Keyboard};
use react::html::{button, div, h2, input, li, p, span, ul};
use react::{
    Element, component, fragment, use_effect, use_id, use_memo, use_reducer, use_ref, use_state,
};

/// Props are a struct; `children` is the element's children.
pub struct CardProps {
    pub title: String,
    pub children: Element,
}

pub fn Card(CardProps { title, children }: CardProps) -> Element {
    div().class_name("card").children((h2().children(title), children))
}

pub struct Todo {
    pub id: u32,
    pub text: String,
    pub done: bool,
}

pub enum Action {
    Add(String),
    Toggle(u32),
}

fn reduce(todos: &Vec<Todo>, action: Action) -> Vec<Todo> {
    match action {
        Action::Add(text) => {
            let mut next: Vec<Todo> = todos.iter().map(|t| Todo { id: t.id, text: t.text.clone(), done: t.done }).collect();
            next.push(Todo { id: todos.len() as u32 + 1, text, done: false });
            next
        }
        Action::Toggle(id) => {
            todos.iter().map(|t| Todo { id: t.id, text: t.text.clone(), done: if t.id == id { !t.done } else { t.done } }).collect()
        }
    }
}

/// A list with keys, a reducer, an input, and a conditional child.
pub fn Todos() -> Element {
    let (todos, dispatch) = use_reducer(reduce, Vec::new());
    let (draft, set_draft) = use_state(String::new());
    let id = use_id();
    let left = use_memo(move || todos.iter().filter(|t| !t.done).count(), (todos,));
    let add = move || {
        if !draft.is_empty() {
            dispatch.dispatch(Action::Add(draft.clone()));
            set_draft.set(String::new());
        }
    };
    component(Card, CardProps {
        title: "Todos".to_string(),
        children: fragment((
            input()
                .id(id.clone())
                .value(draft.clone())
                .on_change(move |e: &Change| set_draft.set(e.value()))
                .on_key_down(move |e: &Keyboard| {
                    if e.key() == "Enter" {
                        add();
                    }
                }),
            button().class_name("add").on_click(move |_| add()).children("Add"),
            ul().children(
                todos
                    .iter()
                    .map(|t| {
                        li().key(t.id)
                            .class_name(if t.done { "done" } else { "" })
                            .on_click(move |_| dispatch.dispatch(Action::Toggle(t.id)))
                            .children(t.text.clone())
                    })
                    .collect::<Vec<_>>(),
            ),
            if todos.is_empty() { Some(p().class_name("empty").children("Nothing to do")) } else { None },
            span().class_name("left").children((left, " left")),
        )),
    })
}

/// State, an effect that runs once and cleans up, and a ref.
pub fn Clock() -> Element {
    let (ticks, set_ticks) = use_state(0);
    let renders = use_ref(0);
    renders.set_current(renders.current() + 1);
    use_effect(
        move || {
            set_ticks.update(|t| t + 10);
            move || set_ticks.set(-1)
        },
        (),
    );
    fragment((
        span().class_name("ticks").children(ticks),
        button().class_name("tick").on_click(move |_| set_ticks.update(|t| t + 1)).children("tick"),
    ))
}

pub fn App() -> Element {
    div().id("app").children((component(Todos, ()), component(Clock, ())))
}
