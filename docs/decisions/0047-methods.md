# 0047. Methods are an object of functions named after their type

Status: Accepted. Extends [0020](0020-structs-and-tuples.md).
Trait-method rejection is superseded by [0049](0049-traits-and-generics.md)
for its supported subset; inherent methods keep this representation.

## Context

rust-js compiled free functions only: an `impl` block was an error. Rust
code keeps its operations with its types, like `Project::empty()` and
`project.opening(path)`. The playground had to write them as free
functions, `opening(project, path)`, to compile (ADR 0044).

A struct is a plain JS object (ADR 0020), and so is a value JS hands over,
like a parsed JSON example. Plain objects have no prototype to put methods
on. Without a class, a JS programmer keeps such functions in a module, or
in an object of them.

## Decision

**An `impl` block's functions are an object named after its type, in the
module the `impl` is in.** Each function is a method of that object, in
shorthand. A method's receiver is its first parameter, named after the type:

```rust
impl Counter {
    pub fn new(step: u32) -> Counter { Counter { count: 0, step } }
    pub fn tick(&mut self) { self.count += self.step; }
}

let mut counter = Counter::new(2);
counter.tick();
```

```js
export const Counter = {
  new(step) {
    return { count: 0, step };
  },
  tick(counter) {
    counter.count = counter.count + counter.step >>> 0;
  }
};

let counter = Counter.new(2);
Counter.tick(counter);
```

- **The JS path is the Rust path.** `Counter::new` is `Counter.new`, and in
  another module, `shapes::Square::new` is `shapes.Square.new`. A method
  call is its fully qualified form: `counter.tick()` is Rust's
  `Counter::tick(&mut counter)`, so it's `Counter.tick(counter)`.
- **Each type's names are its own.** Two types can each have a `new`, and
  a method named like a JS keyword is fine, since it's a property.
- **`self` is named after the type**, `counter` for a `Counter`, as a JS
  function of one would name it. By reference or by value, it's the
  object itself (ADR 0023). `&mut self` changes it in place, as `&mut` to
  an object does (ADR 0025).
- **The object is exported** if any of its methods is `pub` or used by
  another module, as a function is.
- **It comes before the module's `const`s**, whose values (a
  `thread_local!`'s, say) may call its methods as the module loads.
- **An object of one method is laid out like one of several**, the method
  on its own lines, although oxc prints an object of one property on one
  line.
- **A `#![rust_js::camel_case]` crate** (ADR 0046) names methods like its
  functions: `side_length` is `sideLength`.
- **Trait methods, and associated constants, are errors for now.** A trait
  method can be called through a generic or a `dyn`, which needs dispatch
  on the value's type. That's a design of its own.

## Why

- **Plain objects stay plain.** Structs from Rust and objects from JS are
  the same kind of value, with or without methods, as ADR 0020 wants.
- **The names are predictable.** A JS caller of a Rust library finds
  `Counter.new` where the Rust has `Counter::new`, with no mangling.
- **It's what hand-written JS looks like** for operations on data without
  a class: an object of functions, called with the value.

## Alternatives

- **Classes**, with methods on the prototype. A struct would be an instance,
  `new Counter(..)`, and a JSON object couldn't be a `Counter` without
  converting it. Copies (ADR 0020) would need to keep the prototype.
- **Free functions in the module**, `opening(project, path)`. Two types'
  `new` would collide, so names would need a prefix (`Counter_new`) that
  JS callers must guess. `new` itself can't be a function's name.
- **Methods as arrow-function properties**, `new: (step) => {..}`. Same
  object, but it reads less like hand-written code than shorthand does.

## Consequences

- The object is a `const`, so it exists once its module has run. A module
  in a cycle that calls another's methods while loading can find it not
  yet made, as with any `const` (ADR 0031). Functions don't have this.
- A component's module that also has an `impl` exports an object besides
  its components, so it's no longer a Fast Refresh boundary. Types with
  methods belong in their own modules, as in React code.
