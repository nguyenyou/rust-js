# 0081. A `T: Serialize` bound's evidence is `T`'s writer, and `T: DeserializeOwned`'s its reader

Status: Accepted. Extends [0049](0049-traits-and-generics.md) and [0080](0080-serde-generics-and-conversions.md).

## Context

A client talks to its server through a few generic functions: one that
sends any request, one that decodes any response.

```rust
pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}
```

A generic function gets a dictionary for each trait its type parameters
are bound by (ADR 0049). serde's traits had none, so `from_str::<T>` in one
was an error.

## Decision

**The evidence for `T: Serialize` is the function that writes a `T`, and
for `T: Deserialize<'de>` or `T: DeserializeOwned`, the one that reads
one:** what a generic codec takes for its type parameters (ADR 0080), and
named as it names them.

```js
export function decode(text, readT) {
  const result = $fromJson(text, readT);
  return result.TAG === "Err" ? { TAG: "Err", _0: $displayJsonError(result._0) } : result;
}
decode(text, $json.with(pageDeserialize_deserialize, userDeserialize_deserialize));
```

- **A caller passes what it would use itself:** `$json.u32`, a type's
  `deserialize`, or a generic one's with its arguments' readers.
- **It's a function, not a dictionary of one:** a codec and a bound's
  evidence are the same thing, so each passes as it is.
- **With other bounds, each gets its own:** `T: Serialize +
  DeserializeOwned + Debug` is `writeT`, `readT` and `TDebug`.
- serde's impls get no dictionaries (ADR 0052's): their evidence is a codec.

### Also here

- A `serde_json::Error` is a value like any other, in a closure's
  parameter too: `map_err(|e| e.to_string())`.

## Why

- **It's serde's answer.** The `api` example encodes and decodes through
  generic functions, of structs, generic structs and vectors, in a closure
  and with a borrowed `Deserialize<'de>` bound, and each value and error
  matches native Rust.
- **It's the JS a person writes** for a typed fetch: the decoder is an
  argument.
