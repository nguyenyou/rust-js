//! The server's API, as the client calls it: `fetch`, with a method, a body
//! and an abort signal, and the JSON both ways (serde).

use js::{encode_uri_component, settle};
use models::{Contact, NewContact, Problem};
use webapi::{AbortSignal, RequestInit, abort_signal, headers, response, window};

/// Why a request failed.
pub enum Failure {
    /// A newer request replaced it: nothing to show.
    Aborted,
    /// No answer: the server is down, say.
    Network,
    /// The server's answer, and why.
    Refused(u16, Problem),
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Aborted => "cancelled".to_string(),
            Failure::Network => "the server can't be reached".to_string(),
            Failure::Refused(_, problem) => problem.message.clone(),
        }
    }
}

/// The server's answer's text, if it answered with a status of `ok`, or why
/// not. A rejection, a network error or an abort, is an `Err` (`settle`).
async fn call(url: &str, init: RequestInit<'_>, ok: u16) -> Result<String, Failure> {
    let signal = init.signal;
    let response = match settle(window::fetch_with_init(window, url, init)).await {
        Ok(response) => response,
        Err(_) if signal.is_some_and(abort_signal::aborted) => return Err(Failure::Aborted),
        Err(_) => return Err(Failure::Network),
    };
    let status = response::status(response);
    let text = response::text(response).await;
    if status == ok {
        return Ok(text);
    }
    // Not the server's JSON: a proxy's page, say.
    let problem =
        serde_json::from_str(&text).unwrap_or_else(|_| Problem::new(&format!("the server answered {status}")));
    Err(Failure::Refused(status, problem))
}

fn get(signal: &AbortSignal) -> RequestInit<'_> {
    RequestInit {
        signal: Some(signal),
        ..Default::default()
    }
}

/// The contacts matching `query`.
pub async fn contacts(query: &str, signal: &AbortSignal) -> Result<Vec<Contact>, Failure> {
    let text = call(
        &format!("/api/contacts?q={}", encode_uri_component(query)),
        get(signal),
        200,
    )
    .await?;
    serde_json::from_str(&text).map_err(|error| Failure::Refused(200, Problem::new(&error.to_string())))
}

/// The contact `id`.
pub async fn contact(id: u32, signal: &AbortSignal) -> Result<Contact, Failure> {
    let text = call(&format!("/api/contacts/{id}"), get(signal), 200).await?;
    serde_json::from_str(&text).map_err(|error| Failure::Refused(200, Problem::new(&error.to_string())))
}

/// `new`, stored: the contact the server made of it.
pub async fn create(new: &NewContact) -> Result<Contact, Failure> {
    let json = headers::new();
    headers::set(json, "content-type", "application/json");
    let body = serde_json::to_string(new).expect("JSON of a model");
    let init = RequestInit {
        method: Some("POST"),
        headers: Some(json),
        body: Some(&body),
        ..Default::default()
    };
    let text = call("/api/contacts", init, 201).await?;
    serde_json::from_str(&text).map_err(|error| Failure::Refused(201, Problem::new(&error.to_string())))
}
