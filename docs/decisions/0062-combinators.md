# 0062. Combinators and adapters: the closure's body in place

Status: Accepted. Extends [0030](0030-option.md), [0035](0035-results-and-throwing-js.md) and [0036](0036-iterators-and-sorting.md).

## Context

Everyday Rust chains `Option`'s and `Result`'s combinators, iterator
adapters and `Vec`'s methods. rust-js had a few of each (`map`,
`unwrap_or`, `filter`, `fold`, `push`), and the rest were errors:
`unwrap_or_else`, `and_then`, `map_err`, `filter_map`, `zip`,
`max_by_key`, `contains`, `insert`, and so on.

## Decision

**Each is the JS a person writes for it, with a closure whose body is one
value written in place,** as `Option::map` already was:

| Rust | JS |
|---|---|
| `h.unwrap_or_else(\|\| 99)` | `h ?? 99` |
| `h.map_or(7, \|x\| x * 3)` | `h != null ? Math.imul(h, 3) >>> 0 : 7` |
| `h.and_then(half)`, `h.filter(\|&x\| x > 2)` | `h != null ? half(h) : undefined`, `h != null && h > 2 ? h : undefined` |
| `h.ok_or(e)` | `h != null ? { TAG: "Ok", _0: h } : { TAG: "Err", _0: e }` |
| `r.map(f)`, `r.map_err(f)`, `r.and_then(f)` | `r.TAG === "Ok" ? { TAG: "Ok", _0: .. } : r`, … |
| `v.iter().filter_map(f)` | `v.map(f).filter((item) => item != null)` |
| `flat_map`, `flatten`, `chain`, `zip` | `flatMap`, `flat`, `concat`, `$zip` |
| `take_while`, `skip_while`, `step_by(n)` | `$takeWhile`, `$skipWhile`, `.filter((_, i) => i % n === 0)` |
| `max_by_key(f)`, `min_by(f)` | `$maxBy(items, (a, b) => $cmp(key(a), key(b)))`, `$minBy(items, f)` |
| `product()`, `nth(n)`, `find_map(f)`, `partition(p)` | `reduce`, `items[n]`, `map(f).find(..)`, `$partition` |
| `v.contains(&x)` | `v.includes(x)`, or `v.some(..)` with `==` (ADR 0053) |
| `insert`, `remove`, `swap`, `truncate`, `dedup`, `extend` | `$insertAt`, `$removeAt`, `$swap`, `$truncate`, `$dedup`, `$extend` |
| `windows(n)`, `chunks(n)`, `concat()` | `$windows`, `$chunks`, `flat()` |

- **A closure made of statements gets a name first:** `const then = (x) =>
  { .. }; r.TAG === "Ok" ? then(r._0) : r`.
- **Rust's order is kept:**
  - an argument Rust computes whatever happens, like `map_or`'s default,
    goes in a `const` first if it has effects;
  - a closure only runs when Rust would run it, on the side of the `??` or
    `? :` that needs it.
- **What panics in Rust panics here:** `insert` past the end, `remove`
  and `swap` out of bounds, and `windows(0)`. `splice` would clamp
  instead.
- **On a lazy iterator (ADR 0055):** the adapters JS's iterator helpers
  share (`map`, `filter`, `flatMap`, `find`, `reduce`) stay lazy. `zip`,
  `chain`, `take_while` and `skip_while` of one are errors.
- **Not yet:** these on an `Option` of a generic `T`, which may be boxed
  (ADR 0051).

## Why

- **Checked against Rust's own output.** One `report()` formats every
  result with `{:?}` on both sides, and the strings must be equal; out of
  bounds must panic in both.
- **The common case reads as JS.** `h ?? 99` is what a person would
  write, not a call of a helper.
