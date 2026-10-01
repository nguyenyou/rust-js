# 0134. A `let` of a pattern without a value declares each of its variables

Status: Accepted. Extends [0038](0038-js-names-and-destructuring.md).

## Context

A destructuring assignment gives values to variables declared before it,
which a `let` of a pattern can declare: `let (a, b); (a, b) = (1, 2);`. A
`let` of one variable without a value was supported, `let a;`, and a
destructuring assignment was; a `let` of a pattern without a value was an
error.

## Decision

**`let (a, [b, c]);` is a `let` of each of its variables, as `let a; let b;
let c;` is:**

```rust
let (a, b);
(a, b) = (1, 2);
```

```js
let a;
let b;
const [lhs, lhs$1] = [1, 2];
a = lhs;
b = lhs$1;
```

- **Each variable is declared as a `let x;` of its own is**, so one with a
  destructor is the error that `let x;` of one is: it needs its value
  where it's declared.
- **A pattern binding a name to what's inside it, `x @ ..`, without a
  value**, is still an error.

## Why

- **It's exact**: the variables are the same, with the values the
  assignments give them.
- **It's the JS a person writes**: a `let` for each, as for `let a; let b;`.

## Consequences

- `let`s of patterns without values compile, compared with native Rust by
  the `destructuring_assignment` corpus case.
- A destructuring assignment is still the `const` of its value's parts,
  and an assignment of each, as rustc desugars it, not JS's own `[a, b] =
  [1, 2]`.
