# 0051. `Option<T>` in generic code: boxed only when it looks like `None`

Status: Accepted. Extends [0030](0030-option.md) and [0049](0049-traits-and-generics.md).

## Context

`Some(x)` is `x`, and `None` is `undefined` (ADR 0030). That's only right when
`x` can't be `undefined` or `null` itself, so `Option<()>` and
`Option<Option<T>>` are errors.

Generic code can't know. In `fn count<T>(xs: Vec<T>)`, `T` might be `()`,
whose value is `undefined`, or `Option<i32>`, one of whose values is. Then
`Some(x)` would be `undefined`, and would read as `None`. So every
`Option<T>` in generic code was an error, which ruled out ordinary Rust
like `fn first<T>(xs: &[T]) -> Option<&T>`.

`Option<&T>` was worse: the check didn't look through the reference, so it
compiled, and with `T = Option<i32>` a first element of `None` read as
`None`. Concrete code had the same hole: `Some(&())`.

## Decision

**In generic code, `Some` of a type parameter is `$some(x)`: `x` itself,
unless `x` looks like `None`, and then a box.** This is ReScript's approach
for nested options (`Caml_option.some`):

```js
function $some(x) {
  if (x == null) return { $someNone: 0 };                  // Some(None), Some(())
  if (typeof x === "object" && "$someNone" in x) return { $someNone: x.$someNone + 1 };
  return x;                                                // every other value
}
```

- **`None` is still `undefined`**, so "is it `Some`" is still `o != null`:
  a box is an object.
- **Taking the value out is `$someValue(o)`,** which unboxes one level. It
  appears in a `Some(v)` pattern, `?`, `unwrap`, `unwrap_or`
  (`$someValue(o ?? $some(d))`) and `map`.
- **It costs nothing for ordinary values.** `$some(5)` is `5`, so a generic
  function called with numbers, strings or structs returns plain values,
  and a JS caller sees what it would without generics. Only a `None`-like
  value is ever boxed.
- **Only where the payload is a type parameter**, looking through
  references, `Box` and `Rc`, which are the value itself (ADR 0023):
  `Option<T>`, `Option<&T>`, `Option<Box<T>>`. Code that isn't generic
  keeps `Some(x)` as `x`.
- **Std functions that make an `Option` of a generic element box it too:**
  - `Vec::pop` is `$pop(v)`;
  - a slice's `first` and `last`, and an iterator's `find`, are `$someAt(v, i)`;
  - `Result::ok` wraps its value in `$some`.

  Any other std call that would make one is a compile error, not JS that's
  quietly wrong.
- **Where an `Option` is made, a concrete `None`-like payload is an error:**
  at `Some(..)`, and at a std call that returns one. This closes the
  `Some(&())` hole, and keeps a concrete `Option<Option<T>>` an error even
  as a temporary, which the type check of variables didn't see.

A box never reaches code that isn't generic. There, a type whose `Some`
would need one is still an error, so the only place a box can appear is
inside generic code, or in what an exported generic function hands a JS
caller for a `None`-like argument.

## Why

- **Generic code is correct for every `T`.** Checked against native Rust
  with `T = ()` and `T = Option<i32>`, the cases where a plain `Some(x) = x`
  gets it wrong.
- **The output stays plain where it can.** `$some(x)` appears only in
  generic code, and it returns `x` unchanged for anything that isn't
  `None`-like, so interop is unchanged.
- **Proven prior art.** ReScript has represented nested options this way for
  years, and it's the same trade: an allocation only for the rare
  `Some(None)`.

## Alternatives

- **Reject `None`-like instantiations at the call site.** Keep `Some(x)` as
  `x`, and make `count::<()>(..)` an error wherever `T` ends up in an
  `Option`, however deep in generic calls. That needs the compiler to track
  every function's use of each parameter, and it would reject correct Rust.
- **Box every `Some` in generic code.** Simpler, but then a generic
  function returns boxes to its JS callers even for numbers.
- **Lift the rule for concrete code too**, boxing `Option<Option<i32>>`
  everywhere. The same helpers would do it. It's left for when a program
  needs it, since it changes the representation of concrete values.

## Consequences

- An exported generic function called from JS with a `None`-like argument
  hands back a `{ $someNone }` box for `Some` of it.
- `$some`, `$someValue`, `$someAt` and `$pop` are runtime helpers, emitted
  only by modules that use them.
- A last `match` arm that does nothing (`None => {}`) no longer prints an
  empty `else {}`, which this change's example first showed.
