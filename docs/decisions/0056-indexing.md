# 0056. Indexing: `$index(v, i)` to read, `v[$at(v, i)] = x` to write

Status: Accepted. Extends [0020](0020-structs-and-tuples.md) and [0025](0025-vec-loops-refcell-mut.md).

## Context

A slice or an array was read with `a[i]`, as `$index(a, i)`. That helper
checks the index, because Rust panics past the end where JS returns
`undefined`. Nothing else about indexing worked:

- **A `Vec`'s `v[i]` was an error.** For a `Vec`, rustc writes it as a
  call, `*Index::index(&v, i)`, not as built-in indexing.
- **Writing an element, `a[i] = x`, was an error** for arrays, slices and
  `Vec`s alike.
- **`&mut v[i]` wasn't possible,** and nor was a field of one, `v[i].x = 1`.

Writes need a check of their own. JS's `v[5] = x` on a shorter array
makes the array longer, where Rust panics.

## Decision

| Rust | JS |
|---|---|
| `v[i]`, for a `Vec`, a slice or an array | `$index(v, i)` |
| `v[i] = x`, `v[i] += 1` | `v[$at(v, i)] = x`, `v[$at(v, i)] = v[$at(v, i)] + 1 >>> 0` |
| `v[i].x = 5` | `$index(v, i).x = 5` |
| `&v[i]`, `&mut v[i]` | `$index(v, i)`: the element itself (ADR 0025) |
| `v[i].x`, a read | `$index(v, i).x` |

- **`$at(v, i)` checks the index and returns it,** so a write is still an
  assignment, and it panics as Rust does.
- **The array itself is indexed, never a copy of it:** `a[$at(a, 1)] = 9`.
- **Rust runs the right side of `=` first,** and so does the JS.
- **An array that has elements written is a type that changes in place**
  (ADR 0020). A copy of it is `a.slice()`, or a copy of each item that
  changes too. `let b = a; a[1] = 9;` leaves `b` as it was.
- **A `Copy` value read through a reference is copied too,** not just one
  read from a place. `*v.first().unwrap()` isn't `v[0]` itself, and
  neither is `v[i]`. A field read through one copies just the field, if it
  needs a copy: `$unwrap(v[0]).x`.
- **Still errors:** ranges (`&v[1..3]`, whose JS `slice` would be a copy,
  not a view) and indexing a string (ADR 0034).

## Why

- **Out of bounds panics, reading or writing,** as in Rust. A JS array
  would quietly grow or answer `undefined`.
- **It still reads as indexing.** `v[$at(v, i)] = x` is an assignment to an
  element, with the check where it applies.
- **References are the elements.** `&mut v[i]` changes the element in the
  `Vec`, as ADR 0025 has `&mut` do for any object.

## Alternatives

- **`$setIndex(v, i, x)` for writes.** It works the same, but it hides
  the assignment in a call.
- **No check on writes.** Shorter, but an out-of-bounds write would make
  the array longer instead of panicking, which is a different program.
