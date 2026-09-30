# js: the JS language for rust-js

The `js` crate declares what JS has that Rust's `std` doesn't, as ReScript's
standard library does: its promises, errors and regular expressions, its byte
buffers, its JSON, and its global functions. What the browser adds is the
[`webapi`](../webapi/README.md) crate's; what `std` has, rust-js maps itself.
It holds declarations only, so it's never compiled to JS. See
[ADR 0102](../docs/decisions/0102-js-and-webapi.md).

```rust
use js::{decode_uri_component, encode_uri_component, settle, spawn};

let query = encode_uri_component("a b&c");             // encodeURIComponent("a b&c")
let text = decode_uri_component("%E0%A4%A");           // Err: a URIError
spawn(Box::new(async move {                            // runs, unawaited
    match settle(webapi::window::fetch(webapi::window, &url)).await {
        Ok(response) => { .. }                         // what fetch fulfils with
        Err(error) => { .. }                           // a network error: not a throw
    }
}));
```

- A promise is a `Promise<T>`, to `.await` ([ADR 0029](../docs/decisions/0029-async-await.md));
  `settle(p)` makes its `.await` a `Result`, and one a binding declares as
  `Promise<Result<T, &JsError>>` is one already ([ADR 0035](../docs/decisions/0035-results-and-throwing-js.md)).
- `JsError` is what JS threw; `js_error::to_string(e)` is `String(e)`, and
  `js_error::is_error(e)` and `js_error::message(e)` are an `Error`'s.
- `RegExp` is there for what Rust would use `regex` for: `reg_exp::new(r"^\d+$", "")`,
  and `reg_exp::replace(text, pattern, "$1")`. A replacer closure, and `matchAll`,
  are a program's own bindings, typed for its pattern's groups.
- `json::stringify(text)` is a string's JSON, which is a JS string literal too.
  A Rust value's JSON is serde's ([ADR 0077](../docs/decisions/0077-serde-json.md)),
  and so is reading JSON: what `JSON.parse` gives is whatever the text holds.
- `object::from_entries(entries)` is a JS object of keys and values, as an API
  taking a dictionary wants, and `object::is(a, b)` is `Object.is`: whether two
  JS objects are one.
- `number::to_fixed(x, digits)` is `x.toFixed(digits)`, JS's rounding: a tie
  away from zero, `2.5` to `"3"`, where `format!("{:.0}", 2.5)` is `"2"`.

```bash
builtins/build.sh -o "$PWD/target/libjs.rmeta"
./target/debug/rust-js app.rs -- --extern js=target/libjs.rmeta
```
