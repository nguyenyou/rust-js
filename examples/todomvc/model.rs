//! TodoMVC's state: the todos, what changes them, as a reducer, which of
//! them a route shows, and where they're kept.

use js::JsError;

#[derive(Clone, PartialEq)]
pub struct Todo {
    pub id: u32,
    pub title: String,
    pub completed: bool,
}

pub enum Action {
    Add(String),
    Toggle(u32),
    ToggleAll(bool),
    Edit(u32, String),
    Destroy(u32),
    ClearCompleted,
}

pub fn reduce(todos: &Vec<Todo>, action: Action) -> Vec<Todo> {
    match action {
        Action::Add(title) => {
            let id = todos.iter().map(|todo| todo.id).max().unwrap_or(0) + 1;
            let mut next = todos.clone();
            next.push(Todo {
                id,
                title,
                completed: false,
            });
            next
        }
        Action::Toggle(id) => todos
            .iter()
            .map(|todo| Todo {
                completed: todo.completed != (todo.id == id),
                ..todo.clone()
            })
            .collect(),
        Action::ToggleAll(completed) => todos
            .iter()
            .map(|todo| Todo {
                completed,
                ..todo.clone()
            })
            .collect(),
        Action::Edit(id, title) => todos
            .iter()
            .map(|todo| {
                if todo.id == id {
                    Todo {
                        title: title.clone(),
                        ..todo.clone()
                    }
                } else {
                    todo.clone()
                }
            })
            .collect(),
        Action::Destroy(id) => todos.iter().filter(|todo| todo.id != id).cloned().collect(),
        Action::ClearCompleted => todos.iter().filter(|todo| !todo.completed).cloned().collect(),
    }
}

/// Which todos a route shows: `#/`, `#/active` or `#/completed`.
#[derive(Clone, Copy, PartialEq)]
pub enum Filter {
    All,
    Active,
    Completed,
}

impl Filter {
    pub fn from_hash(hash: &str) -> Filter {
        match hash {
            "#/active" => Filter::Active,
            "#/completed" => Filter::Completed,
            _ => Filter::All,
        }
    }

    pub fn shows(self, todo: &Todo) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !todo.completed,
            Filter::Completed => todo.completed,
        }
    }
}

const KEY: &str = "todos-rust-js";

unsafe extern "Rust" {
    // Each may throw, and in a sandboxed frame, as the playground's is,
    // `localStorage` itself does: an `Err` (ADR 0035), and the todos are
    // the page's only.
    #[link_name = "localStorage.getItem"]
    safe fn stored(key: &str) -> Result<Option<String>, &'static JsError>;
    #[link_name = "localStorage.setItem"]
    safe fn store(key: &str, value: &str) -> Result<(), &'static JsError>;
    #[link_name = "JSON.parse"]
    safe fn parse(json: &str) -> Result<Vec<Todo>, &'static JsError>;
    #[link_name = "JSON.stringify"]
    safe fn stringify(todos: &Vec<Todo>) -> String;
}

/// The todos kept from before, or none.
pub fn load() -> Vec<Todo> {
    match stored(KEY) {
        Ok(Some(json)) => parse(&json).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The todos, kept for next time, where they can be.
pub fn save(todos: &Vec<Todo>) {
    let _ = store(KEY, &stringify(todos));
}
