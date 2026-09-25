# 0010. Unique names per function, flat blocks

Status: Accepted

## Context

Rust and JS disagree about names in three ways:

1. **Shadowing.** Rust allows `let x = 1; let x = x + 1;` in one scope. In JS,
   declaring `let x` twice in one block is a `SyntaxError`.
2. **Reserved words.** `new`, `class`, `default` and `this` are fine Rust
   identifiers but not fine JS ones.
3. **Our own names.** We introduce temporaries and reference globals like
   `Math.imul`. Those must never collide with user names.

## Decision

- **Every local gets a name that's unique within its function.** The first
  `x` is `x`, the next distinct variable named `x` is `x$1`, then `x$2`.
  THIR gives each variable its own id (`LocalVarId`), so shadowed variables
  are different ids and simply get different names.
- **`$` marks everything we invent.** Rust identifiers can't contain `$`,
  so a name with `$` can never clash with a user's name. Our temporaries are
  `tmp` and `match`; if a user already has a `tmp`, ours becomes `tmp$1`.
- **Reserved words get a `$` suffix**: `new` → `new$`. The list also
  includes `Math` and `Error`, because the output refers to those globals.
- **Function names are reserved in each function**, so a local never shadows
  a function the code calls.
- **Rust blocks are flattened.** A `{ ... }` block doesn't become a JS block.
  Its statements go straight into the surrounding code. This is safe *because*
  names are unique: nothing inside the block could clash with anything outside.
- **`const` vs `let`**: a binding without `mut` that's initialized right away
  becomes `const`. Otherwise (`mut`, `let x;` assigned later, or initialized
  through statements) it's `let`. Parameters follow the same rule for
  mutability.

## Why

- Unique names solve shadowing, flattening and collisions with one mechanism.
  No scope analysis is needed.
- `const` tells the reader "this never changes", exactly like Rust's default
  immutability. It carries Rust's intent into the JS.
- Flat code is easier to read than nested bare `{}` blocks.

## Alternatives

- **Mirror Rust's scopes with JS blocks**: keeps shadowing legal in more
  cases, but still breaks on same-scope shadowing, and adds brace noise.
- **Name mangling** (`x_12`): collision-proof but unreadable.

## Consequences

- A variable in two sibling blocks gets `x` and `x$1`, even though JS could
  reuse `x`. That's slightly less pretty, and always correct.
- Loop labels have their own namespace and their own uniqueness set (see
  [0015](0015-loops.md)).
