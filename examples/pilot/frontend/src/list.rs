//! The contacts, searched as you type. Each search replaces the last: the
//! one before is aborted, so an answer that comes late never shows.

use crate::api::{self, Failure};
use js::spawn;
use models::Contact;
use react::event::Change;
use react::{Element, use_effect, use_state};
use webapi::abort_controller;

#[derive(Clone)]
enum Loaded {
    Loading,
    Contacts(Vec<Contact>),
    Failed(String),
}

pub fn ContactList() -> Element {
    let (query, set_query) = use_state(String::new());
    let (loaded, set_loaded) = use_state(Loaded::Loading);
    let (attempt, set_attempt) = use_state(0);
    use_effect(
        move || {
            let controller = abort_controller::new();
            let signal = abort_controller::signal(controller);
            let query = query.clone();
            set_loaded.set(Loaded::Loading);
            spawn(Box::new(async move {
                match api::contacts(&query, signal).await {
                    Ok(contacts) => set_loaded.set(Loaded::Contacts(contacts)),
                    Err(Failure::Aborted) => {}
                    Err(failure) => set_loaded.set(Loaded::Failed(failure.message())),
                }
            }));
            move || abort_controller::abort(controller)
        },
        (query, attempt),
    );
    let results = match loaded {
        Loaded::Loading => jsx! { <p className="status">{"Loading…"}</p> },
        Loaded::Failed(message) => jsx! {
            <div className="error" role="alert">
                <p>{message.clone()}</p>
                <button onClick={move |_| set_attempt.update(|n| n + 1)}>{"Try again"}</button>
            </div>
        },
        Loaded::Contacts(contacts) if contacts.is_empty() => jsx! { <p className="status">{"No contacts match."}</p> },
        Loaded::Contacts(contacts) => jsx! {
            <ul className="contacts">
                {contacts
                .iter()
                .map(|contact| {
                    jsx! {
                        <li key={contact.id}>
                            <a href={format!("#/contacts/{}", contact.id)}>{contact.name.clone()}</a>
                            <span className="email">{contact.email.clone()}</span>
                        </li>
                    }
                })
                .collect::<Vec<_>>()}
            </ul>
        },
    };
    jsx! {
        <section>
            <h1>{"Contacts"}</h1>
            <input
                type="search"
                placeholder="Search"
                aria-label="Search"
                value={query.clone()}
                onChange={move |e: &Change| set_query.set(e.value())} />
            {results}
        </section>
    }
}
