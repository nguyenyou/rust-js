# 0035. JS that throws is a `Result`; `?` returns early

Status: Accepted. Builds on [0033](0033-enums-with-fields.md), where a
`Result` is ReScript's `{ TAG: "Ok", _0: v }`.

## Context

JS reports failure by throwing. Rust has no exceptions: it returns a
`Result`, and a panic is a bug, not something to catch. The playground's
compile step needs the difference. A failed compile ends in a WebAssembly
trap, which `main.ts` catches (`try { exit = wasi.start(..) } catch (e) ..`).
Rust code calling JS had no way to say "this may throw", and no `?`.

- **ReScript** catches with `try .. catch { | Exn.Error(e) => .. }`, or wraps a
  call in a `result` itself.
- **Scala.js** has Scala's `try`/`catch` and `js.JavaScriptException`.
- **wasm-bindgen** has `#[wasm_bindgen(catch)]`: a binding returns
  `Result<T, JsValue>`, and a thrown value is the `Err`.

## Decision

**An `extern` function whose Rust result is a `Result` is called in a
`try`**, as with wasm-bindgen's `catch`, but said by the type alone:

```rust
#[link_name = "JSON.parse"]
safe fn parse_numbers(json: &str) -> Result<Vec<u32>, &'static JsError>;
```

```js
const match = $try(() => JSON.parse(json));   // { TAG: "Ok", _0: .. } or { TAG: "Err", _0: e }
```

- **One returning `Promise<Result<T, E>>` settles either way**:
  `$settle(p)` turns a rejection into an `Err`, so its `.await` doesn't throw.
- **`web::JsError` is what was thrown**, usually an `Error`.
  `js_error::to_string(e)` is `String(e)`: `"SyntaxError: JSON Parse error: .."`.
- **`?` returns the `Err` or `None` as it is:**

  | Rust | JS |
  |---|---|
  | `let x = f()?;` on a `Result` | `const result = f(); if (result.TAG === "Err") { return result; }`, then `result._0` |
  | `let a = g()?;` on an `Option` | `const a = g(); if (a == null) { return undefined; }` |

  It's recognized whole, like `.await`: the `match` on `Try::branch(e)`. An `Err`
  whose type changes on the way out is converted with the crate's own `From`
  (ADR 0052): `return { TAG: "Err", _0: appErrorFromString_from(result._0) }`.
  A `From` of std's, like into a `Box<dyn Error>`, is still an error.
- **`Result`'s methods**: `is_ok()`, `is_err()` (`r.TAG === "Ok"`), `ok()` and
  `unwrap_or(d)` (`r.TAG === "Ok" ? r._0 : d`), and `unwrap()`, `expect(msg)`
  (`$unwrapOk`, with Rust's message and the error's `{:?}`).
- **`.clone()` of a type that's never changed in place is the value itself**,
  since nothing can tell the two apart (ADR 0020). A `Result`, a `String`, most
  enums: one object serves as both.

## Why

- **The type says it**: rustc makes the caller handle the `Err`, as with any
  `Result`, and the binding needs no attribute.
- **Throwing stays JS's business**: Rust code never sees an exception, only
  a value. A panic, which is a bug, still throws.
- **The JS reads like JS**: a `try` in one small helper, and early `return`s
  where Rust has `?`.

## Alternatives

- **An attribute on the binding** (wasm-bindgen's `catch`): the same effect,
  but a tool attribute (rejected in ADR 0021), and the type says it anyway.
- **Catch every JS call**: safe, but a `try` around every DOM call, and a
  `Result` that callers of `append` would have to handle.
- **`catch_unwind` around Rust code**: it catches panics, which are bugs, and
  JS exceptions aren't Rust panics.

## Consequences

- A JS function that throws but whose binding doesn't say so still throws,
  as a panic would.
- `$try`'s `Err` is whatever was thrown. `&JsError` is the honest type, since
  JS can throw any value.
- Not yet: `?` with std's `From` conversions, `map`, `map_err`, `and_then`, `ok_or`,
  and `Result`'s other methods.
