//! One todo: its checkbox, its title, and, double-clicked, a field to edit
//! it, saved on Enter or blur, cancelled on Escape, deleted when emptied.

use crate::model::{Action, Todo};
use react::event::{Change, Keyboard};
use react::{Dispatch, Element, jsx, use_ref, use_state};

pub struct TodoItemProps {
    pub todo: Todo,
    pub dispatch: Dispatch<Action>,
}

pub fn TodoItem(TodoItemProps { todo, dispatch }: TodoItemProps) -> Element {
    let (editing, set_editing) = use_state(false);
    let (draft, set_draft) = use_state(todo.title.clone());
    // An edit that's ended, saved or cancelled: the blur of the field that
    // goes with it isn't another.
    let ended = use_ref(false);
    let id = todo.id;
    let save = move || {
        if ended.current() {
            return;
        }
        ended.set_current(true);
        let title = draft.trim().to_string();
        if title.is_empty() {
            dispatch.dispatch(Action::Destroy(id));
        } else {
            dispatch.dispatch(Action::Edit(id, title));
        }
        set_editing.set(false);
    };
    let title = todo.title.clone();
    let edit = move |_: &react::event::Mouse| {
        ended.set_current(false);
        set_draft.set(title.clone());
        set_editing.set(true);
    };
    let keys = move |e: &Keyboard| {
        if e.key() == "Enter" {
            save();
        } else if e.key() == "Escape" {
            ended.set_current(true);
            set_editing.set(false);
        }
    };
    let class = match (todo.completed, *editing) {
        (true, true) => "completed editing",
        (true, false) => "completed",
        (false, true) => "editing",
        (false, false) => "",
    };
    jsx! {
        <li className={class}>
            <div className="view">
                <input
                    className="toggle"
                    type="checkbox"
                    checked={todo.completed}
                    onChange={move |_: &Change| dispatch.dispatch(Action::Toggle(id))} />
                <label onDoubleClick={edit}>{todo.title.clone()}</label>
                <button className="destroy" onClick={move |_| dispatch.dispatch(Action::Destroy(id))} />
            </div>
            {if *editing { Some(jsx! {
                <input
                    className="edit"
                    autoFocus={true}
                    value={draft.clone()}
                    onChange={move |e: &Change| set_draft.set(e.value())}
                    onBlur={move |_| save()}
                    onKeyDown={keys} />
            }) } else { None }}
        </li>
    }
}
