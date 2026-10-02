# 0135. A trait's const parameter is its impl's, and a trait method's is given

Status: Accepted. Extends [0107](0107-const-generics.md) and [0133](0133-impl-names-apart.md).

## Context

A function's, a type's and an impl's const parameters are values their
callers give (ADR 0107); a trait's, `trait Scaled<const K: u32>`, and a
trait method's own, `fn repeat<const N: usize>(&self)`, were errors. In a
default copied into an impl, a const argument could be the trait's or
already the impl's, and its index couldn't tell which.

## Decision

**A trait method's own const parameters are given by its caller after its
arguments, before its own bounds' dictionaries, through a dictionary too;
and a trait's are its impl's arguments for them.**

```rust
trait Repeat { fn repeat<const N: usize>(&self) -> [u32; N]; }
fn three<R: Repeat>(r: &R) -> [u32; 3] { r.repeat::<3>() }

trait Scaled<const K: u32> { fn scaled(&self) -> u32; fn factor(&self) -> u32 { K } }
impl Scaled<10> for Meters { .. }
impl Scaled<100> for Meters { .. }
```

```js
function three(r, RRepeat) {
  return RRepeat.repeat(r, 3);
}

function metersScaled10() {
  ..
  $metersScaled10 = { scaled: metersScaled10_scaled, factor: (self) => 10 };
}
```

- **A dictionary's entry takes a method's own const values from its caller**
  and gives them to the impl's method, after the impl's own; one that
  passes on just what it's given, in order, is the method.
- **A default copied into an impl is given its trait's const parameters'
  values, as the impl's arguments for them**: `10` of `Scaled<10>`, or the
  impl's own `K` of `impl<const K: u32> Scaled<K>`. Its own are given after
  its arguments, as a method's are. Its body names only its trait's and its
  own parameters, so it's given those alone, found before it's lowered,
  where the impl's are known, and no index can be the impl's.
- **An impl of a trait for one type, with two const arguments**, `Scaled<10>`
  and `Scaled<100>`, is named by the value (ADR 0133): `metersScaled10`.
- **Still errors:** a `dyn` of a trait with a const parameter, as of one with
  a type parameter.

## Why

- **It's exact**: the value is the caller's, or the impl's, as rustc's
  monomorphized code has it, compared with native Rust by the
  `const_generic_traits` corpus case.
- **It's the JS a person writes**: a value given as an argument, and a
  constant written into the dictionary that's for it.

## Consequences

- Const generics of traits and their methods compile.
- ADR 0107's "not yet" for them is done.
