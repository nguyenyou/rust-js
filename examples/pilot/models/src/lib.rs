//! What the client and the server agree on: the JSON each sends, and the
//! rules a contact must keep, checked on both sides.

use serde::{Deserialize, Serialize};

/// A contact the server has stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub id: u32,
    pub name: String,
    pub email: String,
    pub age: u32,
}

/// A contact as a form sends it, before the server gives it an id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewContact {
    pub name: String,
    pub email: String,
    pub age: u32,
}

/// What's wrong with one field of a form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// Why the server refused a request: a message, and the fields to blame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub message: String,
    pub errors: Vec<FieldError>,
}

impl Problem {
    pub fn new(message: &str) -> Problem {
        Problem {
            message: message.to_string(),
            errors: Vec::new(),
        }
    }
}

pub const MAX_NAME: usize = 40;

/// Each rule `contact` breaks, in the form's order. The server keeps them
/// as the client does, so a request made without the form is held to them.
pub fn validate(contact: &NewContact) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let mut fail = |field: &str, message: String| {
        errors.push(FieldError {
            field: field.to_string(),
            message,
        });
    };
    let name = contact.name.trim();
    if name.is_empty() {
        fail("name", "a name is required".to_string());
    } else if name.chars().count() > MAX_NAME {
        fail("name", format!("a name is at most {MAX_NAME} characters"));
    }
    if !is_email(&contact.email) {
        fail("email", format!("{:?} isn't an email address", contact.email));
    }
    if contact.age < 13 || contact.age > 130 {
        fail("age", format!("{} isn't an age between 13 and 130", contact.age));
    }
    errors
}

/// Text, an `@`, and a domain with a dot that neither starts nor ends it.
pub fn is_email(text: &str) -> bool {
    match text.split_once('@') {
        Some((user, domain)) => {
            !user.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        None => false,
    }
}

/// Does `contact` match a search: its name or email, ignoring case.
pub fn matches(contact: &Contact, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || contact.name.to_lowercase().contains(&query) || contact.email.to_lowercase().contains(&query)
}
