# 0145. A generic function is given the size and name of a type parameter it asks for

Status: Accepted. Extends [0049](0049-traits-and-generics.md), [0090](0090-wasm32-front-end.md)
and [0098](0098-destructors.md).

## Context

`size_of::<u32>()` is `4`, rustc's answer for the wasm32 target rust-js
checks programs for (ADR 0090), and `type_name::<u32>()` is `"u32"`. Of a
type parameter, they were errors: a generic function is one JS function
for every type, so its `T` has no one answer.

```rust
fn describe<T>(_: &T) -> String {
    format!("{} {}", type_name::<T>(), size_of::<T>())
}
```

Each caller knows its type, as each caller of a function with a destructor
to run knows its drop (ADR 0098).

## Decision

**A generic function that asks `size_of`, `align_of` or `type_name` of a
type parameter takes the answer as a parameter, after its dictionaries,
and each caller gives it for its type:**

```js
function describe(_, TSize, TName) {
  return `${TName} ${TSize}`;
}

describe(1n, 8, "u64");
```

- **Asked directly, in a closure, or through a function it calls:** a
  generic function of the crate's that passes its own `U` on as a callee's
  `T` asks what the callee asks, `relay(u, UClone, USize, UName)` giving
  `describe(u, USize, UName)`. The analysis runs backwards from each ask,
  as `drop_params` runs forwards from each value with a destructor.
- **A fact is one value:** `size_of` and `size_of_val` of a sized type ask
  its size, `align_of` its alignment, `type_name` and `type_name_of_val` its
  name, each once, in that order, after the dictionaries and before any
  drop function.
- **A caller's concrete type answers it,** as rustc computes it for
  wasm32; a caller's own type parameter passes on what it was given.
- **Still errors:**
  - through a trait's dictionary, a trait method's or an impl's, whose
    callers can't see what it asks, and a `Drop` impl's;
  - of an associated type, `T::Native`, or of a type holding a parameter,
    `type_name::<Cow<'a, T>>()`;
  - `size_of_val` of an unsized `T`, which is the value's size;
  - of a library's function its consumers reach (ADR 0100), which never
    see what it asks.

## Why

- **It's exact:** the value native Rust's monomorphized code has, for the
  wasm32 target, compared with native Rust by the `type_facts` corpus case
  on types whose layout is the same on both.
- **It's the JS a person writes:** a number or a string passed in, and a
  function that asks nothing takes nothing.

## Alternatives

- **One object per type parameter, `{ size, align, name }`,** given to
  every generic function. Simpler to pass on, but every generic function
  would take one, most for nothing.
- **A dictionary entry,** as a bound's. A size has no bound to hang on:
  every `T` is `Sized`.

## Consequences

- 5 rustc tests pass that stopped at a size or a name of a type parameter.
