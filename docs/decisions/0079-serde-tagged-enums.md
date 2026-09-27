# 0079. A tagged or untagged enum is read as serde reads one: through the value, read first and kept

Status: Accepted. Extends [0078](0078-serde-json-reading.md).

## Context

ADR 0078 reads an enum when it's externally tagged, `{"Circle": 1.5}`,
where the variant's name comes first. serde's other representations don't
put it first:

```json
{"id": 5, "type": "placed"}        // #[serde(tag = "type")]
{"c": "hi", "t": "Text"}           // #[serde(tag = "t", content = "c")]
1.5                                // #[serde(untagged)]
```

serde reads these by reading the value first into a `Content`, a value of
no particular type, and then reading the variant from it, once the tag has
said which one it is, or trying each variant in turn. That second reading
has its own steps and its own errors: a two-field tuple variant's `[1, 2,
3]` read from the text is `trailing characters`, and from a `Content` it's
`invalid length 3, expected 2 elements in sequence`. An error there has no place in the text,
so at the top it's just its message, and inside a struct it's where the
struct is.

## Decision

**What reads a type reads from either: the text, or a value already read.**
`$JsonReader` (the text) and `$JsonContent` (serde's `ContentDeserializer`)
share `$JsonDecoder`, which has what serde's impls and derives read,
`struct`, `vec`, `map`, `tuple` and the rest, built on a few methods each
one ports: `deserializeNumber`, `deserializeStr`, `deserializeSeq`,
`deserializeMap`, `deserializeAny`, `enum`, `identifier`. The tables a
derive writes (ADR 0078) don't change: `$json.u32` reads from either.

- **A `Content` is `{ type, value }`**, `unit`, `bool`, `num`, `str`, `seq`
  or `map`, what serde_json's `deserialize_any` gives serde's
  `ContentVisitor`.
- **An owned one and a borrowed one differ, as in serde:** an internally
  tagged enum's `ContentDeserializer` takes `{}` for a unit, and an untagged
  enum's `ContentRefDeserializer` doesn't. `owned` tells them apart.
- **A key in a `Content` is a string**, read as any other value: a
  `BTreeMap<u32, _>` in an internally tagged variant can't be read, as in
  serde.

**Each representation is its derive, ported:**

| Rust | JS |
|---|---|
| `#[serde(tag = "type")]` | `json.internallyTagged("type", .., (variant, content) => ..)` |
| `#[serde(tag = "t", content = "c")]` | `json.adjacentlyTagged("t", "c", .., (variant, content) => ..)` |
| `#[serde(untagged)]` | `json.untagged(message, [(content) => .., ..])` |
| `#[serde(untagged)]` on the last variants | `json.untagged(message, [(content) => content.enum(..), ..])` |

- **Internally tagged**: the tag is found among the other fields, which are
  kept (serde's `TaggedContentVisitor`), and the variant reads what's left.
  A unit variant ignores it, a newtype variant reads its type from it.
- **Adjacently tagged**: the tag then the content, or the content first,
  kept until the tag comes; `[tag, content]` too; a duplicate, an unknown
  field under `deny_unknown_fields`, and a missing content, which is `None`
  for a newtype of an `Option`, as serde's derive has them.
- **Untagged**: the value is kept, and each variant reads it in turn; the
  first that doesn't fail is the value, and if none does, it's `data did
  not match any variant of untagged enum ..`.
- **A struct's `tag`** is only written: reading, it's a field like any
  other the struct doesn't have, as in serde.

## Why

- **It's serde's answer.** The `inbox` example reads each representation
  from right and wrong JSON, with the tag or the content first, missing,
  twice, of the wrong type, in an array, inside a struct and an untagged
  enum, and every value and error matches native Rust, with its place or
  without. `wire` reads its own enums back.
- **Ported, not reinvented:** serde's two ways of reading a variant, and
  the errors each gives, are what the server's own reading does.

## Alternatives

- **Reading the text twice** for an untagged enum, once per variant, from
  where it starts: no `Content`, but the errors would be the text's, not
  serde's, and a variant that reads part of it and fails would move the
  reader.
- **Looking for the tag first** in an internally tagged object: simpler,
  but it would find a mistake before or after serde does, and so tell a
  different one.

## Consequences

- An untagged enum reads its value once and each variant from it, as serde
  does, so a big one is kept whole while its variants are tried.
