//! One contact, loaded by its id, or why it can't be.

use crate::api::{self, Failure};
use js::spawn;
use models::Contact;
use react::{Element, jsx, use_effect, use_state};
use webapi::abort_controller;

#[derive(Clone)]
enum Loaded {
    Loading,
    Contact(Contact),
    Failed(String),
}

pub struct ContactPageProps {
    pub id: u32,
}

pub fn ContactPage(ContactPageProps { id }: ContactPageProps) -> Element {
    let (loaded, set_loaded) = use_state(Loaded::Loading);
    use_effect(
        move || {
            let controller = abort_controller::new();
            let signal = abort_controller::signal(controller);
            set_loaded.set(Loaded::Loading);
            spawn(Box::new(async move {
                match api::contact(id, signal).await {
                    Ok(contact) => set_loaded.set(Loaded::Contact(contact)),
                    Err(Failure::Aborted) => {}
                    Err(Failure::Refused(404, _)) => {
                        set_loaded.set(Loaded::Failed(format!("There's no contact {id}.")))
                    }
                    Err(failure) => set_loaded.set(Loaded::Failed(failure.message())),
                }
            }));
            move || abort_controller::abort(controller)
        },
        (id,),
    );
    match loaded {
        Loaded::Loading => jsx! { <p className="status">{"Loading…"}</p> },
        Loaded::Failed(message) => jsx! { <p className="error" role="alert">{message.clone()}</p> },
        Loaded::Contact(contact) => jsx! {
            <article className="contact">
                <h1>{contact.name.clone()}</h1>
                <dl>
                    <dt>{"Email"}</dt>
                    <dd>{contact.email.clone()}</dd>
                    <dt>{"Age"}</dt>
                    <dd>{contact.age}</dd>
                </dl>
                <a href="#/">{"All contacts"}</a>
            </article>
        },
    }
}
