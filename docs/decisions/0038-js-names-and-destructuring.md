# 0038. Variables have JS's names and shapes: `const [count, setCount] = ..`

Status: Accepted.

## Context

A React component written by hand starts like this:

```js
const [count, setCount] = useState(0);
```

The same Rust, `let (count, set_count) = use_state(0);`, came out as

```js
const tmp = useState(0);
// .. tmp[0] .. tmp[1] ..
```

That has three differences from hand-written JS. The name keeps Rust's snake
case, the tuple is indexed instead of taken apart, and a closure's parameter
can't reuse a name from outside: `(count$1) => count$1 + 1`. None of this is
wrong, but it's the first thing a JS reader sees. ReScript prints the names
you wrote, and its `let (a, b) = ..` is also `tmp[0]`.

## Decision

**A variable's JS name is its Rust name in camelCase.** `set_count` is
`setCount`, `any_negative` is `anyNegative`. Leading and trailing
underscores stay (`_unused`, `type_`), and so does a name with no lowercase
letter. Only variables and parameters are renamed. Functions, fields and
exports keep their Rust names, because other JS code uses them by name. A
crate can choose camelCase for those too, with `#![rust_js::camel_case]`
([0046](0046-camel-case-crates.md)).

**A tuple or struct pattern of plain variables is JS destructuring**, in a
`let` of a computed value and in a parameter:

| Rust | JS |
|---|---|
| `let (count, set_count) = use_state(0);` | `const [count, setCount] = useState(0);` |
| `let (mut lo, hi) = f();` | `let [lo, hi] = f();` |
| `let (_, only) = f();` | `const [, only] = f();` |
| `fn Card(CardProps { title, children }: CardProps)` | `function Card({ title, children })` |
| `fn swap((a, b): (i32, i32))` | `function swap([a, b])` |

A pattern with anything else in it, or a part whose type needs its own copy
(ADR 0020), is taken apart as before. So is a `let` of a place, which still
names the place (`p.x`) without copying it.

Two more follow from reading the output:

- **A closure's names are its own.** A parameter or local can reuse an outer
  name, `setCount((count) => count + 1)`, unless the closure captures what
  that name holds. The module's own names (functions, imports, globals) are
  never reused.
- **A closure's trailing `_` parameters are left out**, because JS ignores
  extra arguments: `|_| ..` is `() => ..`.
- `!(a === b)` is `a !== b`.

## Why

- **It's how JS code is written.** React's docs, and every component a React
  user has read, use camelCase and destructuring.
- **Destructuring is exact.** It reads each part once, as `tmp[0]` did, and
  a copy is still made where ADR 0020 needs one.
- **Scoping names to the closure is what JS does.** Only a captured name has
  to stay visible inside, and that's the one rule kept.

## Alternatives

- **Keep Rust's names** (ReScript's choice). Nothing to explain, but every
  React component would read `set_count`.
- **camelCase everything, fields and functions too.** Then `pub fn fetch_data`
  becomes `fetchData`, and JS code calling a Rust library has to guess the
  name. Fields are data that JS reads by name as well.

## Consequences

- Every program's output changed where a name had an underscore. Tests pin
  the new names.
- Two Rust variables that differ only by underscores (`set_count`,
  `setcount`) are fine: the second gets `$1`, as any clash does.
