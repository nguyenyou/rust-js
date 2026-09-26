# 0059. `HashMap` is a JS `Map`, `HashSet` a `Set`, keyed by value

Status: Accepted. Extends [0036](0036-iterators-and-sorting.md).

## Context

`HashMap` and `HashSet` were errors, and they're among the most used types
in std. JS has `Map` and `Set`, but they compare keys differently. A JS
`Map` finds a key with `SameValueZero`: by value for numbers, strings and
booleans, by identity for objects. Rust finds one with the key's `Hash`
and `Eq`. The two agree only where JS compares by value.

## Decision

**A `HashMap<K, V>` is a `Map`, and a `HashSet<K>` a `Set`, when `K` is
one JS compares by value:** a number (not `f64`, which isn't `Hash`
anyway), a string, a `char`, a `bool`, or a fieldless enum, which is a
string (ADR 0013). Any other key is a compile error.

| Rust | JS |
|---|---|
| `HashMap::new()`, `HashSet::new()` | `new Map()`, `new Set()` |
| `m.insert(k, v);`, `s.insert(x);` | `m.set(k, v)`, `s.add(x)` |
| `let old = m.insert(k, v)` | `$insert(m, k, v)`: the old value, or `undefined` |
| `m.get(&k)`, `m.contains_key(&k)`, `m[&k]` | `m.get(k)`, `m.has(k)`, `$unwrap(m.get(k), "key not found")` |
| `m.remove(&k);`, `s.remove(&x)` | `m.delete(k)`, `s.delete(x)` |
| `m.len()`, `m.is_empty()` | `m.size`, `m.size === 0` |
| `*m.entry(k).or_insert(0) += 1` | `m.set(k, (m.get(k) ?? 0) + 1)` |
| `m.entry(k).or_default().push(x)` | `$orInsert(m, k, []).push(x)` |
| `for (k, v) in &m` | `for (const [k, v] of m)` |
| `m.iter()`, `keys()`, `values()`, `into_iter()` | `Array.from(m)`, … (arrays, ADR 0036) |
| `collect()`, `HashMap::from(pairs)` | `new Map(pairs)` |
| `m.clone()`, `Default::default()` | `new Map(m)`, `new Map()` |
| `{:?}` | `{"a": 1, "b": 2}` |

- **A result that's used gets a helper; one that isn't doesn't.**
  `insert` and `remove` return the old value in Rust, and a set's
  `insert` whether the value was new. As statements they're plain
  `m.set(k, v)` and `s.add(x)`. When the value is used they're `$insert`,
  `$remove` and `$add`, which return it.
- **A value in the map is written through `set`:**
  `*m.get_mut(&k).unwrap() += 1` is `m.set(k, $unwrap(m.get(k)) + 1)`. A
  value that's an object, like a `Vec`, is changed in place as any
  reference is (ADR 0025).
- **A clone clones the values that need it** (ADR 0052), not the keys,
  which are primitives.
- **Still errors:**
  - `==` on maps (`$eq` compares objects by their fields, and a `Map` has
    none);
  - `BTreeMap`;
  - `entry` used other than as `m.entry(k).or_…`.

### Also here

- `a += b` with a `&u32` `b`, as in `for x in &v { sum += x }`, is the
  same `sum = sum + x`.
- `Option<&T>`'s `copied()` and `cloned()` are a clone of the value.
- `#[derive(Hash)]` no longer makes a crate an error: a derived impl's
  generic `hash<H>` is never lowered, so it's no longer checked.

## Why

- **It's the JS a person writes.** A `Map` is what JS code uses for a
  dictionary, and `counts.set(word, (counts.get(word) ?? 0) + 1)` is the
  JS way to count.
- **It's exact where it applies.** Where `SameValueZero` and Rust's `Eq`
  agree, a `Map` does what a `HashMap` does. Elsewhere it's an error.
- **Order.** A `HashMap`'s order is arbitrary in Rust, and a `Map`'s is
  the order keys were inserted, so any code that works in Rust works
  here.

## Alternatives

- **Keys as strings, `JSON.stringify(key)`,** which would allow any key.
  But every access would convert, and the JS would read like encoding
  rather than a map.
- **Hashing with the key's `Hash` and `Eq`,** as Rust does. It's exact
  for any key, but it needs a hash table in the runtime, and it would
  replace JS's own `Map` for the keys that need it least.
- **An object, `{}`, for string keys.** `__proto__` and friends make it
  unsafe for arbitrary keys, and it can't hold numbers as numbers.
