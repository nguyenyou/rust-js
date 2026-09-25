# 0002. Generate JS from THIR, not MIR

Status: Accepted

## Context

rustc lowers a program through several representations:

```
AST ──► HIR ──► THIR ──► MIR ──► LLVM IR
        names   types    control-flow graph
        resolved on every (basic blocks + gotos)
                node
```

We have to pick one to read from.

## Decision

Read **THIR** (Typed High-level IR).

## Why

Think of MIR as a flowchart and THIR as an outline.

- **MIR is a flowchart**: numbered boxes joined by `goto`. `if`, `while` and
  `match` are gone, dissolved into jumps. JS has no `goto`, so to print JS
  from MIR we would first have to *rediscover* the loops and ifs
  ("structuring"). The result reads like machine output.
- **THIR is still an outline**: it has `If`, `Loop`, `Match`, `Block` and
  `Let`, shaped like the source. That shape is what makes readable JS
  possible. Every node also carries its fully inferred type, and the work we
  don't want to redo is already done: method calls are resolved into plain
  `Call`s, and implicit conversions are explicit.
- **HIR** is also tree-shaped, but its types live in a separate side table
  and method calls aren't resolved yet. We'd be redoing THIR's work.

ReScript makes the same trade: it prints JS from OCaml's high-level IR, not
from the low-level one.

## Alternatives

- **MIR**: see above. Also, borrowck consumes it, and the codegen interface
  only gives us monomorphized MIR, one copy per generic instantiation, where
  JS would happily take one copy.
- **HIR + typeck results**: works, but duplicates THIR's lowering.

## Consequences

- THIR still contains some desugarings. For example `while c { .. }` arrives
  as `loop { if c { .. } else { break } }`. We turn it back where it matters
  for readability. See [0015](0015-loops.md).
- THIR wraps many nodes in `Scope`, `Use` and `NeverToAny` nodes that don't
  change meaning. `strip()` in `lower.rs` skips them when pattern-matching
  shapes.
- THIR is only available on nightly, through `rustc_private`. See [0003](0003-pin-nightly-toolchain.md).
- rustc discards ("steals") THIR when it builds MIR. See [0004](0004-driver-hook.md).
