//! The builtins crate (ADR 0102) as a program calls it: each function is the
//! JS global or method it names. For the test in compiler.test.ts.

use js::{json, reg_exp};

/// A string's JSON text, which is a JS string literal too.
pub fn quoted(text: &str) -> String {
    json::stringify(text)
}

/// `pattern`'s matches replaced with `with`, in which `$1` is the first group.
pub fn replaced(text: &str, pattern: &str, flags: &str, with: &str) -> String {
    reg_exp::replace(text, reg_exp::new(pattern, flags), with)
}

/// A JS object of these keys and values, as an API taking a dictionary wants.
pub fn attributes(label: &str, level: u32) -> &'static js::JsObject {
    js::object::from_entries(vec![("aria-label".to_string(), label.to_string()), ("aria-level".to_string(), level.to_string())])
}

/// `Object.is`: the same object, or the same value, NaN too.
pub fn identical(same: bool) -> (bool, bool) {
    let a = js::object::from_entries(vec![("k".to_string(), 1)]);
    let b = if same { a } else { js::object::from_entries(vec![("k".to_string(), 1)]) };
    (js::object::is(a, b), js::object::is(&f64::NAN, &f64::NAN))
}

unsafe extern "Rust" {
    #[link_name = "Promise.reject"]
    safe fn rejected(reason: &str) -> js::Promise<Result<u32, &'static js::JsError>>;
}

/// What was thrown: an `Error`'s message, or that it's no `Error`.
pub async fn thrown() -> (String, String) {
    let error = js::decode_uri_component("%E0%A4%A").unwrap_err();
    let first = if js::js_error::is_error(error) { js::js_error::message(error) } else { "not an Error".to_string() };
    let rejected = rejected("no").await.unwrap_err();
    let second = if js::js_error::is_error(rejected) { js::js_error::message(rejected) } else { "not an Error".to_string() };
    (first, second)
}

/// `x.toFixed(digits)`, as JS rounds, and `format!`'s, as Rust does.
pub fn fixed(x: f64, digits: usize) -> (String, String) {
    (js::number::to_fixed(x, digits as u32), format!("{x:.digits$}"))
}
