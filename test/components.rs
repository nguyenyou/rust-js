// React components (ADR 0041), compiled to JSX (ADR 0040) and rendered by
// React itself in test/react.test.ts.

#![allow(non_snake_case)]

use react::event::{Change, Keyboard};
use react::jsx;
use react::{
    Context, Element, Memo, create_context, memo, memo_with, use_context, use_effect, use_id, use_memo,
    use_reducer, use_ref, use_state,
};

/// Props are a struct; `children` is the element's children.
pub struct CardProps {
    pub title: String,
    pub children: Element,
}

pub fn Card(CardProps { title, children }: CardProps) -> Element {
    jsx! {
        <div className="card">
            <h2>{title}</h2>
            {children}
        </div>
    }
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
            let mut next: Vec<Todo> = todos
                .iter()
                .map(|t| Todo {
                    id: t.id,
                    text: t.text.clone(),
                    done: t.done,
                })
                .collect();
            next.push(Todo {
                id: todos.len() as u32 + 1,
                text,
                done: false,
            });
            next
        }
        Action::Toggle(id) => todos
            .iter()
            .map(|t| Todo {
                id: t.id,
                text: t.text.clone(),
                done: if t.id == id { !t.done } else { t.done },
            })
            .collect(),
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
    jsx! {
        <Card title={"Todos".to_string()}>
            <>
                <input
                    id={id.clone()}
                    value={draft.clone()}
                    onChange={move |e: &Change| set_draft.set(e.value())}
                    onKeyDown={move |e: &Keyboard| {
                        if e.key() == "Enter" {
                            add();
                        }
                    }} />
                <button className="add" onClick={move |_| add()}>{"Add"}</button>
                <ul>
                    {todos
                    .iter()
                    .map(|t| {
                        jsx! {
                            <li
                                key={t.id}
                                className={if t.done { "done" } else { "" }}
                                onClick={move |_| dispatch.dispatch(Action::Toggle(t.id))}>
                                {t.text.clone()}
                            </li>
                        }
                    })
                    .collect::<Vec<_>>()}
                </ul>
                {if todos.is_empty() { Some(jsx! {
                    <p className="empty">{"Nothing to do"}</p>
                }) } else { None }}
                {todos.last().map(|t| jsx! {
                    <p className="latest">{t.text.clone()}</p>
                })}
                <span className="left">
                    {left}
                    {" left"}
                </span>
            </>
        </Card>
    }
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
    jsx! {
        <>
            <span className="ticks">{ticks}</span>
            <button className="tick" onClick={move |_| set_ticks.update(|t| t + 1)}>{"tick"}</button>
        </>
    }
}

thread_local! {
    /// A context and memoized components: each a `const` of the module.
    static THEME: Context<&'static str> = create_context("light");
    static BADGE: Memo<BadgeProps> = memo(Badge);
    /// Any two labels that aren't empty count as the same props.
    static LOOSE_BADGE: Memo<BadgeProps> = memo_with(Badge, |a, b| a.label.is_empty() == b.label.is_empty());
}

unsafe extern "Rust" {
    /// Counts renders, for the test.
    #[link_name = "globalThis.rendered"]
    safe fn rendered(label: &str);
}

pub struct BadgeProps {
    pub label: &'static str,
}

pub fn Badge(BadgeProps { label }: BadgeProps) -> Element {
    rendered(label);
    let theme = use_context(&THEME);
    jsx! {
        <span className={format!("badge {theme}")}>{label}</span>
    }
}

/// Context from a provider, or its default outside one; `memo` skipping renders.
pub fn Themed() -> Element {
    let (dark, set_dark) = use_state(false);
    let (clicks, set_clicks) = use_state(0);
    jsx! {
        <>
            <BADGE label="outside" />
            <THEME value={if *dark { "dark" } else { "light" }}>
                <>
                    <BADGE label="inside" />
                    <LOOSE_BADGE label={if clicks % 2 == 0 { "even" } else { "odd!" }} />
                </>
            </THEME>
            <button className="theme" onClick={move |_| set_dark.update(|d| !d)}>{"theme"}</button>
            <button className="click" onClick={move |_| set_clicks.update(|c| c + 1)}>{clicks}</button>
        </>
    }
}

pub fn App() -> Element {
    jsx! {
        <div id="app">
            <Todos />
            <Clock />
            <Themed />
        </div>
    }
}
