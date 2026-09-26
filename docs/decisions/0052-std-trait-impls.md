# 0052. The crate's own `Default`, `From` and `Clone`, and the trait ABI kept

Status: Accepted. Extends [0049](0049-traits-and-generics.md) and [0020](0020-structs-and-tuples.md).

## Context

ADR 0049 gave traits a JS shape: a dictionary per impl, dictionaries as
extra arguments to generic functions, and `{ value, impl }` for `dyn`.
Only the crate's own traits, plus `Copy` and a hand-written `Default`,
could be implemented. Every other std trait impl was an error, and
ordinary Rust uses a few of them all the time: `#[derive(Default)]`,
`impl From<f64> for Meters`, and a `Clone` that isn't just a copy.

`.clone()` was also wrong where it compiled. A clone of a `Vec` was the
same array, so after `let w = v.clone(); v.push(3);`, `w` had three items
too. A clone of a struct holding a `Vec` shared the `Vec` the same way.

Once JS code calls generic Rust, the shapes from 0049 are an API that JS
callers depend on. So before adding std traits, this ADR settles them.

## Decision

### The shapes 0049 chose are kept

| | JS | Rule |
|---|---|---|
| An impl's dictionary | `circleShape()` | Lazy and cached. Named after the type, then the trait, then the trait's arguments: `metersFromF64`. Two impls whose names collide are an error. |
| A dictionary | `{ area, name }` | A plain object of the trait's methods. A std trait's has only its required methods: `{ clone }`, `{ default }`. |
| A generic function | `total(shapes, TShape)` | The dictionaries come after the value arguments, in the order the bounds are written, `where` clauses included. |
| A trait object | `{ value, impl }` | |
| A generic impl | `vecShape(TShape)` | Cached by its dictionary arguments, so the same arguments give the same object. |

### `Default`

- **A hand-written impl is a call:** `Config::default()` is `configDefault_default()`.
- **A derived impl is written in place:** a struct becomes an object of its
  fields' defaults, `{ size: 0, tags: [], mode: "Off" }`. An enum becomes
  its `#[default]` variant, read from the constructor that the derived body
  names.
- **In generic code it's the dictionary's `default`:** `TDefault.default()`.

### `From` and `Into`

- **An impl's `from` is a function named like the others:** `impl From<u32>
  for Meters` gives `metersFromU32_from`. The trait's argument is in the
  name because a type usually has more than one `From`.
- **`x.into()` is the same call** as the `from` it resolves to.
- **`From` has no dictionaries.** A generic `T: From<U>` stays an error:
  there is nothing to call through yet.

### `Clone`

**A clone is the value itself unless something could tell the two apart.**
That happens in only two cases:

- One of the two can change in place:
  - a `Vec` of a type that something takes `&mut` of (`push`, `sort`,
    `v[i] = x` all do);
  - an array or a cell;
  - a struct whose fields get assigned (ADR 0020's rule).
- Cloning runs code: a hand-written `clone`, or a generic `T`'s, which
  might be hand-written.

Everything else is shared, as it is today. When a copy is needed:

- **A derived impl copies the parts that need it:**
  - a struct is `{ ...s, tags: s.tags.slice() }`;
  - an enum copies only the variants that hold such a part:
    `m.TAG === "List" ? { ...m, _0: m._0.slice() } : m`;
  - a `Vec` whose items need a copy is `items.map((item) => ..)`.
- **A hand-written impl is a call:** `trackedClone_clone(a)`. A derived impl
  calls it too, for a field that has one.
- **In generic code, `T: Clone` takes a dictionary:** `twice(x, TClone)`
  calls `TClone.clone(x)`. A `T: Copy` copies with its `Copy` dictionary.
- **An iterator's `cloned()` and `copied()` clone each item** that needs it.
- **What isn't supported is an error:** `clone_from`, and cloning a std type
  rust-js doesn't model.

## Why

- **The shapes from 0049 hold up.** A JS caller of a generic function sees
  its normal arguments first. Dictionaries are plain objects that JS can
  build, and impl names are predictable. Changing any of this later would
  break callers, so it's fixed here.
- **Clones stay cheap and readable.** Most clones are of values nothing
  changes, and those stay `const t = s`. The rule for `Vec` is the one ADR
  0020 uses for structs, applied per type across the crate.
- **A hand-written `Clone` or `Default` means what it says.** It runs,
  whether it's called directly, from a derived impl, or through a generic.

## Alternatives

- **Copy every clone.** `structuredClone` or a deep copy is simple and
  always correct, but every `.clone()` of a string or a read-only `Vec`
  would allocate, and the output would be full of copies.
- **Dictionaries for `From`.** `T: From<U>` would need a dictionary per
  argument type, and it's rare in the code rust-js compiles. It can come
  later without changing anything here.
- **Track `Vec` changes per variable, not per type.** That would be more
  precise, but it's the last-use analysis ADR 0020 already puts off.

## Consequences

- Every generic function with a `T: Clone` bound takes a `TClone` argument.
- A `Vec` clone is a real copy wherever its type is changed anywhere in the
  crate. The playground's `files.clone()` is `files.slice()`, because it
  pushes to other `Vec<String>`s.
- `examples/std_traits.rs` is checked against native Rust, including
  clones that would share if they weren't copied.
