# 0082. `#[serde(flatten)]` is serde's flat map: a field's entries among its struct's

Status: Accepted. Extends [0077](0077-serde-json.md), [0078](0078-serde-json-reading.md) and [0079](0079-serde-tagged-enums.md).

## Context

An API shares parts between its types: a page's `page` and `total` beside
each listing's own fields, or the fields a client doesn't know, kept in a
map.

```rust
#[derive(Serialize, Deserialize)]
pub struct Listing {
    pub name: String,
    #[serde(flatten)]
    pub meta: Meta,
    #[serde(flatten)]
    pub extra: BTreeMap<String, u32>,
}
```

`{"name":"a","page":1,"total":9,"x":5}`: the meta's fields and the map's
entries are the listing's own. serde writes them through its
`FlatMapSerializer`, and reads them through its `FlatMapDeserializer`,
which each have their own rules and errors.

## Decision

**Writing, a flattened field is `json.flat(value, write)`:** its writer
writes into a `$jsonFlat` of the object, serde's `FlatMapSerializer`. A
struct's or a map's entries are the object's; a variant is an entry of its
name (`"Film": 7`); `None` and `()` are nothing; and a number, a string or
an array is serde's error, `can only flatten structs and maps (got an
integer)`.

- **Its writer tells the kinds apart,** as serde's `Serializer` does:
  `json.bool`, `json.int`, `json.null`, `json.char`, `json.variant`, and
  `json.beginTuple` and `json.beginTupleStruct` beside `json.beginArray`.

**Reading, the struct keeps what its own fields don't read,** and each
flattened field, in its turn, reads from what's kept (`$JsonFlat`, serde's
`FlatMapDeserializer`):

```js
json.struct("struct Listing", [["name", $json.string]], ([name, meta, extra]) => ({ name, meta, extra }), {
  flatten: [metaDeserialize_deserialize, $json.map($json.key.string, $json.u32)],
});
```

- **A struct takes the entries it has fields for,** a map, an untagged or
  an internally tagged enum sees every one left without taking it, and an
  externally tagged enum takes the first that names one of its variants.
- **An `Option` is `None`** when what it holds can't be read from what's
  left, as serde's is.
- **Anything else is serde's error,** `can only flatten structs and maps`,
  or `no variant of enum Kind found in flattened data`.
- **The struct is read as a map, as serde reads it:** an array is an error,
  and under `deny_unknown_fields`, an entry no one took is `unknown field`.

## Why

- **It's serde's answer.** The `api` example flattens structs, maps,
  options, externally and internally tagged enums, a struct that flattens
  another, and what can't be flattened, both ways, and each value and
  error matches native Rust.
- **It reads as the type does:** the table has the struct's own fields,
  and the flattened ones after.

## Consequences

- A flattened map sees every entry still left, those a flattened struct
  after it takes too; and under `deny_unknown_fields`, what only a map read
  is still unknown. Both are serde's.
