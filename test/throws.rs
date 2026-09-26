//! JS that throws (ADR 0035): an `extern` function whose result is a
//! `Result` is called in a `try`, and a `Promise<Result<..>>` settles either
//! way. For the test in compiler.test.ts; it can't run as native Rust.

use web::{JsError, Promise, js_error};

unsafe extern "Rust" {
    #[link_name = "JSON.parse"]
    safe fn parse_numbers(json: &str) -> Result<Vec<u32>, &'static JsError>;
    #[link_name = "Promise.reject"]
    safe fn rejected(reason: &str) -> Promise<Result<u32, &'static JsError>>;
    #[link_name = "Promise.resolve"]
    safe fn resolved(value: u32) -> Promise<Result<u32, &'static JsError>>;
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
