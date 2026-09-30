//! A new contact: checked as the server will check it (`models::validate`)
//! before it's sent, then the server's answer, its errors by field.

use crate::api::{self, Failure};
use crate::route::go;
use crate::sonner::toast;
use js::spawn;
use models::{FieldError, NewContact, validate};
use react::event::{Change, Event};
use react::{Element, jsx, use_state};

/// The message for `field`, if one of `errors` is about it.
fn message(errors: &[FieldError], field: &str) -> Option<String> {
    errors
        .iter()
        .find(|error| error.field == field)
        .map(|error| error.message.clone())
}

pub fn NewContactForm() -> Element {
    let (name, set_name) = use_state(String::new());
    let (email, set_email) = use_state(String::new());
    let (age, set_age) = use_state(String::new());
    let (errors, set_errors) = use_state(Vec::<FieldError>::new());
    let (sending, set_sending) = use_state(false);
    let submit = move |e: &Event| {
        e.prevent_default();
        // Every field's error at once: an age that isn't a number is one.
        let parsed = age.trim().parse::<u32>().ok();
        let new = NewContact {
            name: name.clone(),
            email: email.clone(),
            age: parsed.unwrap_or(0),
        };
        let mut found = validate(&new);
        if parsed.is_none() {
            found.retain(|error| error.field != "age");
            found.push(FieldError {
                field: "age".to_string(),
                message: format!("{age:?} isn't a whole number"),
            });
        }
        if !found.is_empty() {
            set_errors.set(found);
            return;
        }
        set_errors.set(Vec::new());
        set_sending.set(true);
        spawn(Box::new(async move {
            match api::create(&new).await {
                Ok(contact) => {
                    toast(&format!("Added {}", contact.name));
                    go(&format!("#/contacts/{}", contact.id));
                }
                Err(Failure::Refused(_, problem)) if !problem.errors.is_empty() => set_errors.set(problem.errors),
                Err(failure) => toast(&failure.message()),
            }
            set_sending.set(false);
        }));
    };
    let field = |label: &'static str, key: &'static str, value: String, set: Box<dyn Fn(String)>| {
        let error = message(errors, key);
        jsx! {
            <label className="field">
                <span>{label}</span>
                <input
                    name={key}
                    value={value}
                    aria-invalid={error.is_some()}
                    onChange={move |e: &Change| set(e.value())} />
                {error.map(|text| jsx! { <span className="field-error">{text}</span> })}
            </label>
        }
    };
    jsx! {
        <form onSubmit={submit} noValidate={true}>
            <h1>{"New contact"}</h1>
            {field("Name", "name", name.clone(), Box::new(move |v| set_name.set(v)))}
            {field("Email", "email", email.clone(), Box::new(move |v| set_email.set(v)))}
            {field("Age", "age", age.clone(), Box::new(move |v| set_age.set(v)))}
            <button type="submit" disabled={*sending}>{if *sending { "Adding…" } else { "Add contact" }}</button>
        </form>
    }
}
