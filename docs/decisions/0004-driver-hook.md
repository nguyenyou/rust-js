# 0004. Copy THIR before analysis, stop before codegen

Status: Accepted

## Context

rustc's driver lets a tool run code at fixed points through the `Callbacks`
trait: `after_crate_root_parsing`, `after_expansion` and `after_analysis`.

The catch: when rustc builds MIR for the borrow checker, it *steals* the
THIR, taking it out of its cache for good. By the time `after_analysis`
runs, every function's THIR is gone, and reading it panics.

## Decision

Do everything in `after_expansion` (`src/main.rs`):

```
after_expansion:
  1. for every fn: tcx.thir_body(fn) ──► clone it     (type check runs on demand here)
  2. tcx.ensure_ok().analysis(())                    (borrowck steals THIR; we have copies)
  3. if no errors: lower the copies, write the .js
  4. return Compilation::Stop                        (no LLVM, no .rlib, no dep-info)
```

## Why

- **Step 1 before step 2** is the whole trick: take our copy before rustc
  destroys the original. `Thir` derives `Clone`, and rustc's source comments
  that this is deliberate: it exists for external tools that need THIR after
  it's been stolen.
- **Running analysis ourselves**, instead of returning and waiting for
  `after_analysis`, keeps steps 1–3 in one function. The copied `Thir<'tcx>`
  borrows from the compiler's `'tcx` lifetime, so it can't easily be stored
  across callbacks.
- **Checking `has_errors()`** after analysis matters: hard errors abort
  inside `analysis`, but errors from `#[deny]` lints don't.
- **`Compilation::Stop`**: we only want the front end. Stopping here also
  means rustc writes no files of its own.

## Alternatives

- **`after_analysis`**: too late, THIR is already stolen.
- **Overriding the `thir_body` query provider** to stash a copy: works, but
  more invasive and more coupled to query internals than a clone.
- **Emitting before analysis**: fast, but we'd produce JS for programs the
  borrow checker rejects. See [0006](0006-errors-and-unsupported-features.md).

## Consequences

- Each function's THIR exists twice in memory for a while. That's fine at
  this scale.
- `thir_body` fails (returns `Err`) for functions that don't type-check. We
  skip those, and the analysis step reports the real error.
