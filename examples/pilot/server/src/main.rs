//! The pilot's API, native Rust: contacts in memory, as JSON over HTTP/1.1,
//! checked by the rules the client checks (`models::validate`).
//!
//!   GET  /api/contacts?q=ada&delay=200   the contacts matching `q`
//!   GET  /api/contacts/2                 one contact, or 404
//!   POST /api/contacts                   a `NewContact`: 201, 400, 409 or 422
//!
//! `delay` holds a response back, in milliseconds, so the client's handling
//! of a slow, stale answer can be seen. `PORT` is where it listens, 0 for
//! any port; it prints the address.

use models::{Contact, FieldError, NewContact, Problem, matches, validate};
use serde::Serialize;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Response {
    status: u16,
    body: String,
}

fn json(status: u16, value: &impl Serialize) -> Response {
    Response {
        status,
        body: serde_json::to_string(value).expect("JSON of a model"),
    }
}

fn problem(status: u16, message: &str) -> Response {
    json(status, &Problem::new(message))
}

fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("a port to listen on");
    println!("listening on http://{}", listener.local_addr().expect("an address"));
    let contacts = Arc::new(Mutex::new(vec![
        Contact {
            id: 1,
            name: "Ada Lovelace".into(),
            email: "ada@example.com".into(),
            age: 36,
        },
        Contact {
            id: 2,
            name: "Alan Turing".into(),
            email: "alan@example.com".into(),
            age: 41,
        },
        Contact {
            id: 3,
            name: "Grace Hopper".into(),
            email: "grace@example.com".into(),
            age: 85,
        },
    ]));
    for stream in listener.incoming().flatten() {
        let contacts = Arc::clone(&contacts);
        std::thread::spawn(move || {
            if let Err(error) = serve(stream, &contacts) {
                eprintln!("connection: {error}");
            }
        });
    }
}

fn serve(mut stream: TcpStream, contacts: &Mutex<Vec<Contact>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
    );
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let response = route(&method, &target, &body, contacts);
    let reason = match response.status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        422 => "Unprocessable Content",
        _ => "Unknown",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        response.status,
        response.body.len(),
        response.body
    )
}

fn route(method: &str, target: &str, body: &[u8], contacts: &Mutex<Vec<Contact>>) -> Response {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| decode(value))
    };
    match (method, path.strip_prefix("/api/contacts")) {
        ("GET", Some("")) => {
            if let Some(delay) = param("delay").and_then(|ms| ms.parse().ok()) {
                std::thread::sleep(Duration::from_millis(delay));
            }
            let query = param("q").unwrap_or_default();
            let found: Vec<Contact> = contacts
                .lock()
                .unwrap()
                .iter()
                .filter(|c| matches(c, &query))
                .cloned()
                .collect();
            json(200, &found)
        }
        ("GET", Some(id)) => {
            let id = id.strip_prefix('/').and_then(|id| id.parse::<u32>().ok());
            match contacts.lock().unwrap().iter().find(|c| Some(c.id) == id) {
                Some(contact) => json(200, contact),
                None => problem(404, "no such contact"),
            }
        }
        ("POST", Some("")) => {
            let new: NewContact = match serde_json::from_slice(body) {
                Ok(new) => new,
                Err(error) => return problem(400, &format!("invalid JSON: {error}")),
            };
            let errors = validate(&new);
            if !errors.is_empty() {
                return json(
                    422,
                    &Problem {
                        message: "the contact isn't valid".to_string(),
                        errors,
                    },
                );
            }
            let mut contacts = contacts.lock().unwrap();
            if contacts.iter().any(|c| c.email.eq_ignore_ascii_case(&new.email)) {
                let errors = vec![FieldError {
                    field: "email".into(),
                    message: format!("{} is taken", new.email),
                }];
                return json(
                    409,
                    &Problem {
                        message: "a contact has that email".to_string(),
                        errors,
                    },
                );
            }
            let id = contacts.iter().map(|c| c.id).max().unwrap_or(0) + 1;
            let contact = Contact {
                id,
                name: new.name.trim().to_string(),
                email: new.email,
                age: new.age,
            };
            contacts.push(contact.clone());
            json(201, &contact)
        }
        (_, Some(_)) => problem(405, "not a method this path takes"),
        _ => problem(404, "no such path"),
    }
}

/// A query parameter's value, `%XX` and `+` decoded.
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let hex = |i: usize| {
        bytes
            .get(i..i + 2)
            .and_then(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], hex(i + 1)) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 2;
            }
            (byte, _) => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
