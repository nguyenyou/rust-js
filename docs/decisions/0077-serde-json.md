# 0077. `#[derive(Serialize)]` is a function that writes serde_json's text

Status: Accepted. Extends [0052](0052-std-trait-impls.md) and [0060](0060-debug.md).

## Context

A full-stack Rust app shares its types between the server and the client:

```
 shared.rs  ── #[derive(Serialize, Deserialize)] struct Order { .. }
   │                                         │
   ▼ rustc                                   ▼ rust-js
 server (axum)  ◄──── JSON over HTTP ────►  client (JS)
   serde_json                                 ???
```

The server writes JSON with serde_json. For the two ends to agree, the
client has to write the very same text: the same names after
`rename_all`, the same enum tags, the same number layout. And it has to be
written from the same source, `#[serde(..)]` attributes and all, or the
two will drift.

serde's derive makes a generic `serialize` that drives a `Serializer`
through a visitor. That code is traits, associated types and generics all
the way down: none of it is JS a person would write, and most of it is
what rust-js rejects.

## Decision

**The crate uses real serde.** `serde/build.sh` builds serde, serde_derive
and serde_json with rust-js's pinned toolchain, and prints the `--extern`
flags that find them. rustc checks the shared crate just as the server's
build does, so a mistake in an attribute is rustc's error.

**What serde's derive generates is skipped; what it means is read from the
attributes.** Every item the derive expands to is left out, and its
`Serialize` impl's `serialize` becomes a function that writes the value
with `$jsonWriter`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub order_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
serde_json::to_string(&order)
```

```js
function orderSerialize_serialize(order, json) {
  json.beginObject();
  json.key("orderId");
  json.raw(String(order.order_id));
  if (order.note != null) {
    json.key("note");
    json.string(order.note);
  }
  json.endObject();
}
$toJson(order, orderSerialize_serialize, false);
```

- **It's named and pruned as a derived `Debug` is** (ADR 0060): one
  function per type, emitted only when something serializes it.
- **`serde_json::to_string` and `to_string_pretty`** are `$toJson`. It
  returns a `Result`, whose error is a `serde_json::Error`:
  `{ message, line, column }`, shown as serde_json shows it.
- **The attributes are read before rustc drops them.** HIR keeps no trace
  of `#[serde(..)]`, which only serde_derive reads, so rust-js reads them
  from the expanded AST, by the span of the item's name.
- **The attributes supported** are `rename`, `rename_all`,
  `rename_all_fields`, `tag`, `content`, `untagged`, `transparent`,
  `skip`, `skip_serializing` and `skip_serializing_if`. `rename_all`'s
  rules are serde_derive's own, ported as they are. Those that only change
  deserializing (`default`, `alias`, `deny_unknown_fields`, ..) are
  accepted. Any other (`flatten`, `with`, ..) is an error, as is a generic
  type deriving `Serialize`.
- Field skips apply to tuple structs and tuple variants as well as named
  fields. Custom skip predicates use rustc's resolved function from the
  derive, preserving module scope and import aliases.
- A variant's `untagged` overrides its container's tagging. Named structs
  include their own `tag` and serialized type name. Internally tagged
  newtype variants unwrap transparent/newtype payloads before writing fields.

**Each type is written as serde writes it:**

| Rust | JSON |
|---|---|
| `bool`, integers | `true`, `7` |
| `f64` | serde_json's layout: `1.0`, `1e-7`, `1e16`; `NaN` is `null` |
| `String`, `&str`, `char` | a JSON string |
| `Option<T>` | `null`, or the `T` |
| `Vec`, slices, arrays, sets, tuples | an array |
| `BTreeMap`, `HashMap` | an object; a number or `bool` key is quoted |
| unit struct, `()` | `null` |
| newtype, `transparent` | what it holds |
| tuple struct | an array |
| enum | externally tagged, or as `tag`, `tag` + `content`, `untagged` say |

A float's text is the one thing JS and serde_json write differently:
`String(1)` is `"1"`, where serde_json's is `1.0`, and they switch to an
exponent at different sizes. `$jsonNumber` takes JS's shortest digits,
which are the same digits, and lays them out as serde_json does.

## Why

- **It's serde_json's answer.** The example serializes every tagging,
  renames, skips, maps, escapes and floats (`NaN`, `-0.0`, `1e16`), and
  its compact and pretty text match native serde_json byte for byte.
- **One source.** The attributes the server's derive reads are the ones
  the client's JS is written from.
- **It's what a person writes** to put a type into JSON by hand, one key at
  a time, with nothing in between.

## Alternatives

- **Compiling serde's generated code.** Faithful by construction, but it's
  a trait-driven visitor: pages of JS for each type, and generics rust-js
  doesn't support.
- **`JSON.stringify` of the JS value.** Short, but the JS value isn't the
  JSON: an enum is `{ TAG, _0 }`, an `Option` is `undefined`, a map is a
  `Map`, and the names are Rust's.
- **A schema shared by both ends** (OpenAPI, protobuf): a second source of
  truth for types Rust already has.

## Consequences

- The playground can't load proc macros, so a crate that derives
  `Serialize` compiles only with the native compiler.
- A `HashMap` is written in JS's insertion order, not Rust's hash order.
  Both are valid JSON objects, but not the same text; use a `BTreeMap`
  when the text must match.
- Reading JSON back (`serde_json::from_str`) is still an error.

`test/serde.test.ts` compiles the same source natively and through rust-js,
then compares compact and pretty JSON. It covers these attribute combinations
and predicate resolution independently of generated-code snapshots.
