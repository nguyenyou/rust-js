# 0121. A map keyed by a struct is a `Map` keyed by its value

Status: Accepted. Extends [0059](0059-hashmap.md).

## Context

ADR 0059 makes a `HashMap` a JS `Map`, and refuses any key JS doesn't
compare by value: a `HashMap<Point, u32>`, a `HashSet<(i32, i32)>`, a key
that's an enum with fields or an `Option`. A JS `Map` finds an object key
by identity, so `m.get(Point { x: 1, y: 2 })` would never find the
`Point { x: 1, y: 2 }` put in it.

Programs key maps this way all the time: a grid's cells by `(x, y)`, an
edge by its two ends, a record by a composite id. Rust finds the key by its
`Hash` and `Eq`. When both are derived, `Eq` compares field by field, as
`$eq` does already for `==` of those types.

## Decision

**A `HashMap` or `HashSet` whose key compares by value, field by field, is a
`$KeyMap` or `$KeySet`: a `Map` or `Set` that keeps each entry under a
string of the key's value.** The JS that uses it is a `Map`'s, as it is:

```js
const seen = new $KeySet();
seen.add([1, 2]);
seen.has([1, 2]); // true
const cells = new $KeyMap();
cells.set({ x: 1, y: 2 }, "wall");
cells.get({ x: 1, y: 2 }); // "wall"
for (const [cell, kind] of cells) { .. } // the key as it was put in
```

- **The key's string is `$key(k)`**: a number as it's written, a BigInt with
  `n`, a string quoted, `undefined` and `null` as one, an array its items',
  and an object its fields', by name, in their names' order. Two keys have
  the same string exactly when `$eq` finds them equal, which is what a
  derived `Eq` is (ADR 0052). rust-js builds a struct's fields in their
  declared order, from a literal and from JSON alike, so the order of
  names is for `$eq`'s sake, which doesn't depend on it either.
- **A key compares by value** when it's a number (not `f64`), a `bool`, a
  `char`, a string, a fieldless enum, or a tuple, an array, a slice, a `Vec`,
  a `Box` or an `Option` of those, or a struct or an enum of the crate's
  whose `PartialEq` is derived and whose fields' types are those too.
- **ADR 0059's keys stay a plain `Map`'s and `Set`'s**: a string, a number,
  a `bool` or a fieldless enum is its own key already, and a `$KeyMap` of
  them would only be slower. So is an `Option` of one, `undefined` or the
  value (ADR 0030): `HashMap<Option<u32>, _>` is a `Map`.
- **A set read from JSON**, an array, is a `$KeySet` of its items, so two
  equal ones are one, as serde_json's `HashSet` has them.
- **A clone clones the keys too**, where they need it (ADR 0052): a clone
  that shared them would share objects a consumed map's owner may change.
- **Still errors:**
  - a key whose `PartialEq` is the crate's own, which `$key` would ignore;
  - a `BTreeMap` or `BTreeSet` of these keys: their order is the key's
    `Ord`, fields in declared order, which a string of names in their order
    isn't;
  - a map of these keys in JSON, as serde_json refuses a key that isn't a
    string.

## Why

- **It's the JS a person writes, but for one word**: `new $KeyMap()`, then
  `get`, `set`, `has` and `for..of` as for any `Map`. What a `$KeyMap` is
  doing is in one place, the runtime, not at each use.
- **It's exact where it applies**: a derived `Eq` is `$eq`, and `$key` is
  `$eq` made a string. Elsewhere it's an error, as ADR 0059's are.
- **The helpers are a `Map`'s**: `$insert`, `$orInsert` and the rest call
  `get`, `set` and `has`, which a `$KeyMap` has.

## Alternatives

- **`JSON.stringify(key)` at each use**, a `Map` of strings: every `get` and
  `set` would read as encoding, and iterating would give strings, not keys.
  A struct's fields are in the order it was built, `Point { y, x }`, which
  JSON keeps, so equal keys could have different strings.
- **A hash table in the runtime, by the key's `Hash`**, as Rust does: exact
  for any key, a custom `PartialEq` too, but `Hash` would have to be run as
  Rust runs it, and a `Map` already finds strings.
- **A string key per type, written by the compiler** (`(p) => p.x + "," +
  p.y`): shorter strings, but a function for each key type, and `$key` is
  what `$eq` is already, in one place.

## Consequences

- `HashMap<(i32, i32), _>`, `HashSet<Point>`, and keys that are enums with
  fields or `Option`s compile.
- Each `get`, `set` and `has` of a `$KeyMap` makes the key's string: the
  cost of finding a key by its value.
