# 0080. A generic type's codec takes its type parameters' codecs; `Result`, `from`, `try_from` and `into` are serde's

Status: Accepted. Extends [0077](0077-serde-json.md), [0078](0078-serde-json-reading.md) and [0052](0052-std-trait-impls.md).

## Context

An API's responses are often generic: a `Page<T>` of whatever was asked
for, a `Reply<T>` that's the data or an error. Their derived codecs were an
error. So was a `Result` in a field, and a type that's written as another
type, `#[serde(into = "Raw")]`, or read from one and checked on the way,
`#[serde(try_from = "String")]`.

## Decision

**A generic type's `serialize` takes a writer for each of its type
parameters, and its `deserialize` a reader,** named after the parameter:

```rust
#[derive(Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<T>,
}
serde_json::from_str::<Page<User>>(text)
```

```js
function pageSerialize_serialize(page, json, writeT) { .. writeT(item, json); .. }
function pageDeserialize_deserialize(json, readT) {
  return json.struct("struct Page", [["items", $json.vec(readT)], ["next", $json.option($json.some(readT))]], ..);
}
$fromJson(text, $json.with(pageDeserialize_deserialize, userDeserialize_deserialize));
```

- **It's what serde's generic impl does,** with the `T: Serialize` it's
  bound by made a function: a `Page<u32>` and a `Page<User>` are written
  and read by the same code.
- **An `Option<T>` of a parameter** may hold a boxed `Some` (ADR 0051), so
  it's written through `$someValue`, and read into `$some`.
- **`$json.with(deserialize, ..readers)`** is the reader of a generic type,
  given its arguments' readers.

**`Result<T, E>`** is written and read as serde's impl does, as an enum,
`{"Ok": ..}` or `{"Err": ..}`.

**`#[serde(from = "T")]`, `try_from` and `into`** are serde's: a clone of
the value, converted with the crate's own `From` impl and written as a `T`;
a `T`, read and converted; or tried, where an `Err` is serde's
`Error::custom` of it, its `Display`. The `T` is the one rustc worked out
in the derive's own call of `From::from`, `TryFrom::try_from` or
`Into::into`, so a path or an import in the attribute means what it means
to the server.

### Also here

- **The crate can implement `TryFrom`,** as it can `From` (ADR 0052), with
  its `Error` type: `Even::try_from(n)` and `n.try_into()` call it.

## Why

- **It's serde's answer.** The `api` example writes and reads generic
  structs, enums of every representation, a generic of a generic, a
  `Result`, and types converted to and from what's on the wire, and each
  value and error matches native Rust.
- **One function per type, not per use:** a type's codec is written once,
  however many types it's used with, as the server's is.

## Alternatives

- **A codec per instantiation** (`pageOfUser_deserialize`): no parameters,
  but as many copies as there are uses, and none for a JS caller to reuse.
- **A dictionary per type** (ADR 0049): what generic functions take, but a
  codec needs only the one function, and a reader passes as it is.

## Consequences

- A generic function that writes or reads its own type parameter,
  `fn get<T: DeserializeOwned>(..)`, is still an error: its `T` would need
  its reader passed in.
