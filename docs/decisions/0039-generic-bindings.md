# 0039. Generic bindings: `#[rust_js::link_name]` on an ordinary function

Status: Accepted. Revisits the tool attribute that ADRs 0021 and 0028
rejected.

## Context

A binding in an `extern` block can't be generic. rustc rejects type
parameters, and `impl Trait` in arguments, on foreign items (E0044). React's
API is generic all through:

```rust
pub fn use_state<T>(initial: T) -> (&'static T, SetState<T>);
pub fn use_effect<C: Cleanup>(effect: impl Fn() -> C + 'static, deps: impl Deps);
impl Element { pub fn children(self, children: impl Node) -> Element; }
```

Other things were missing too:

- A method whose receiver is itself the function: `setCount(1)`.
- A function passed as a value: `component(Card, props)`.
- An import that's only there for its side effects: `import "./App.css"`.
- A default import named the way JS code names it: `import heroImg from "./hero.png"`.

ReScript's externals are generic as a matter of course
(`external useState: (unit => 'state) => ('state, ..) = "useState"`), because
they're ordinary declarations with an attribute. Scala.js's facades are
ordinary classes and methods with `@js.native`.

## Decision

**A function or method with `#[rust_js::link_name = ".."]` is a binding**,
with the forms `#[link_name]` has (ADRs 0021, 0024, 0028). It can be generic.
Its body is never compiled, so it's `unreachable!()`:

```rust
#[rust_js::link_name = "react#useState"]
pub fn use_state<T>(initial: T) -> (&'static T, SetState<T>) {
    unreachable!()
}

impl<T> SetState<T> {
    #[rust_js::link_name = "this()"]
    pub fn set(&self, value: T) {
        unreachable!()
    }
}
```

- **rust-js registers the tool** for every crate it compiles, passing
  `-Zcrate-attr=feature(register_tool, custom_inner_attributes)` and
  `-Zcrate-attr=register_tool(rust_js)`, so a program never writes them. A
  library of bindings that plain rustc builds, like `react`, writes them once
  in its `lib.rs`. rustc keeps tool attributes in a crate's metadata, so they
  work across crates.
- **A method's `self` is `this`**, as a first parameter named `this` is in
  an `extern` block.
- **`this()`**: call `this` itself. `set_count.set(1)` is `setCount(1)`.
- **`()` given to JS is `[]`**, the empty tuple (ADR 0020). This is only for
  an argument, since a function returning `()` still returns `undefined`.
  So `use_effect(f, ())` is `useEffect(f, [])`.
- **A function used as a value is its JS name.** `component(Card, props)`
  passes `Card`.
- **`#![rust_js::import = "./App.css"]`** in a module is `import "./App.css";`
  in its file.
- **A default import held by one `static` is named after it:**
  `#[link_name = "./assets/hero.png#default"] static hero_img: &'static str`
  is `import heroImg from "./assets/hero.png"`. When nothing names it, it's
  still named after the module (ADR 0028).

## Why

- **Types are the point of a binding**, and React's types are generic. A
  generic `fn` gets them from rustc's own checking, with nothing new to learn.
- **ADR 0021 rejected tool attributes** because every program would need two
  feature lines. rust-js adding them itself removes that cost.
- **Each extra form is what hand-written JS has**: `setCount(1)`, `useEffect(f, [])`,
  `import "./App.css"`.

## Alternatives

- **Non-generic `extern` bindings, one per type** (`use_state_i32`, ..). Not
  practical for a library, and still no closures or tuples of mixed types.
- **`&dyn Any` for every value.** Type-checks nothing.
- **Recognize React's functions by path in the compiler.** That would work
  for one library only, where a binding works for any.

## Consequences

- rust-js needs a nightly rustc, which it already does (`rustc_private`),
  and so do libraries of generic bindings, as `web` already does.
- A binding's body must still type-check, which is why it's `unreachable!()`.
