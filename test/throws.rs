//! JS that throws (ADR 0035): an `extern` function whose result is a
//! `Result` is called in a `try`, and a `Promise<Result<..>>` settles either
//! way. For the test in compiler.test.ts; it can't run as native Rust.

use js::{JsError, Promise, decode_uri_component, encode_uri_component, js_error, settle};

unsafe extern "Rust" {
    #[link_name = "JSON.parse"]
    safe fn parse_numbers(json: &str) -> Result<Vec<u32>, &'static JsError>;
    #[link_name = "Promise.reject"]
    safe fn rejected(reason: &str) -> Promise<Result<u32, &'static JsError>>;
    #[link_name = "Promise.resolve"]
    safe fn resolved(value: u32) -> Promise<Result<u32, &'static JsError>>;
    #[link_name = "Promise.reject"]
    safe fn rejects(reason: &str) -> Promise<u32>;
}

/// The numbers' sum, or what `JSON.parse` threw.
pub fn sum_json(json: &str) -> Result<u32, String> {
    match parse_numbers(json) {
        Ok(numbers) => {
            let mut sum = 0;
            for n in numbers {
                sum += n;
            }
            Ok(sum)
        }
        Err(e) => Err(js_error::to_string(e)),
    }
}

/// `?` on a thrown error: it's an `Err` like any other.
pub fn first_twice(json: &str) -> Result<u32, &'static JsError> {
    let numbers = parse_numbers(json)?;
    let mut first = 0;
    for n in numbers {
        first = n * 2;
        break;
    }
    Ok(first)
}

/// A rejected promise is an `Err` at its `.await`, not a throw.
pub async fn settled(fail: bool) -> String {
    let result = if fail { rejected("no").await } else { resolved(7).await };
    match result {
        Ok(n) => n.to_string(),
        Err(e) => "rejected: ".to_string() + &js_error::to_string(e),
    }
}

/// The JS language's own globals, from the js crate (ADR 0102):
/// `encodeURIComponent`, and `decodeURIComponent`, which throws a
/// `URIError` on what isn't an encoded text: an `Err`.
pub fn uri(text: &str) -> (String, Result<String, String>) {
    let encoded = encode_uri_component(text);
    let decoded = decode_uri_component(&encoded).map_err(js_error::to_string);
    match decode_uri_component("%E0%A4%A") {
        Ok(_) => (encoded, decoded),
        Err(e) => (encoded + " " + &js_error::to_string(e), decoded),
    }
}

/// A promise of the webapi crate's, `fetch`'s say, rejects with what went
/// wrong; `js::settle` makes its `.await` an `Err` of it.
pub async fn settle_either(fail: bool) -> String {
    let promise = if fail { rejects("no") } else { later(8) };
    match settle(promise).await {
        Ok(n) => n.to_string(),
        Err(e) => "rejected: ".to_string() + &js_error::to_string(e),
    }
}

unsafe extern "Rust" {
    #[link_name = "Promise.resolve"]
    safe fn later(value: u32) -> Promise<u32>;
}
