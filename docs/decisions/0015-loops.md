# 0015. Loops: put `while` back, label only when needed

Status: Accepted

## Context

THIR has one loop construct, `Loop`. Rust's `while` is desugared before we see it:

```
while c { body }   ══►   loop { if c { body } else { break } }
```

A loop is also an expression: `loop { .. break v; }` produces `v`. And
`break`/`continue` can target outer loops by label (`'outer`).

## Decision

1. **Re-sugar `while`.** `as_while()` recognizes exactly that desugared
   shape (a loop whose body is only `if c { .. } else { break }`, with that
   `break` targeting this loop and `c` simple) and prints `while (c) { .. }`.
   Every other loop is `while (true) { .. }`.
2. **Break values use the loop's destination.** Each loop remembers the
   `dest` it was lowered with ([0008](0008-expression-and-statement-modes.md)).
   `break v` delivers `v` there, then breaks. If that destination is
   `Return`, the delivery *is* a `return`, and the `break` is skipped:

   ```rust
   loop { if i == n { break a; } ... }      // loop value is returned
   ```
   ```js
   while (true) { if (i === n) { return a; } ... }
   ```
3. **Labels only when needed.** A JS label is added only when a
   `break`/`continue` jumps past the innermost loop. Its name comes from the
   Rust label (`'outer` → `outer`), or `loop` if unlabeled, made unique per
   function.
4. `continue` maps to `continue`. In a re-sugared `while`, JS re-checks the
   condition, just like Rust.

## Why

- `while (i < n)` is what a human writes, and what the Rust source said.
- Passing `dest` into breaks avoids the temporary-plus-break-plus-return
  dance for the most common "search loop" pattern.
- Unused labels are noise. Labels are created lazily, at first use, so they
  never appear otherwise.

## Alternatives

- **Always `while (true)` with `if (!c) break;`**: correct, less readable.
- **`for (;;)`**: same meaning as `while (true)`. Chose `while (true)` for plainness.

## Consequences

- `as_while` depends on the exact desugaring shape in this rustc version. If
  rustc changes it, loops still compile correctly as `while (true)`, just
  less prettily. That's a safe failure.
- Labeled *blocks* (`'a: { .. break 'a v; }`) aren't supported yet. JS has
  labeled blocks, so this is straightforward future work.
- `for` loops need iterators, which need traits and methods. That's future work.
