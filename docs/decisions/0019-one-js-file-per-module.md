# 0019. One JS file per Rust module

Status: Accepted. Supersedes the "functions inside modules" limit of
[0016](0016-crate-shape.md) and the single output file of
[0005](0005-input-and-output.md).

## Context

Real programs are split across files. How does ReScript do it? Checked in
its compiler source (rescript-lang/rescript at `5b00bcf`):

- **A file is a module**, named after the file: `test_utils.res` is
  `Test_utils`. Source code has no import statements; it just writes
  `Test_utils.eq(..)`.
- **The compiler writes the imports**: that becomes
  `import * as Test_utils from "./test_utils.mjs"`, then `Test_utils.eq(..)`.
  Each `.res` compiles to its own JS file, which ends with `export { .. }`.
- The build tool finds files by module name, so names must be unique in a
  project: *"Duplicate module name … Rename one of these files."*
  Module dependencies must not form cycles.

Rust differs in ways that matter:

| | ReScript | Rust |
|---|---|---|
| Unit of compilation | a file | a crate: rustc sees every file at once |
| Declaring a module | implicit (the file exists) | explicit: `mod math;` |
| Names | flat, globally unique | a tree: `a::util` and `b::util` coexist |
| Cycles between modules | forbidden | allowed within a crate |
| Visibility | everything, unless `.resi` limits it | `pub`, private; a child may use its parent's private items |

rust-js already gets the whole crate in one rustc run, including files
pulled in by `mod math;`, with THIR for every function. What's left is
deciding the shape of the JS output.

## Decision

**Every Rust module with functions becomes one ES module**, following
ReScript's file-per-module model:

```
src/lib.rs          ──► out/lib.js           (the output file given)
src/math.rs         ──► out/math.js          crate::math
src/math/stats.rs   ──► out/math/stats.js    crate::math::stats
mod helpers { .. }  ──► out/helpers.js       inline modules too
```

- **Paths**: the crate root goes to the output file (`-o`, or `<input>.js`).
  Module `a::b` goes to `a/b.js` next to it, mirroring how Rust lays out
  `src/`. A module whose path would collide with the root's file is an error.
- **Cross-module calls** become namespace imports, as in ReScript:
  `math::add(x, y)` ⟶ `import * as math from "./math.js"` at the top and
  `math.add(x, y)` at the call. The alias is the module's last path segment
  (the crate name for the root), made unique within the file, and reserved
  so no local variable can shadow it.
- **Exports**: a function is exported if it's `pub`, *or* if another module
  calls it. The second case matters because a child module may call its
  parent's private function, and in JS that call crosses a file.
- **Cycles are fine.** ES modules allow cyclic imports, and rust-js emits
  only function declarations, which are hoisted, so nothing runs at import
  time that could see a half-initialized module.
- **Source maps**: each JS file gets its own map, pointing into the `.rs`
  file its module lives in.
- **Modules without functions** (only enums, say) produce no file. Nothing
  imports them: fieldless enums are strings ([0013](0013-fieldless-enums.md)).

## Why

- **Readable at the file level**: the JS tree mirrors the Rust tree, so a
  reader finds `math.add` in `math.js`, as they'd find `math::add` in `math.rs`.
- **Namespace imports mirror Rust paths**: `math.add` reads like `math::add`,
  and two modules can each have an `add` without renaming.
- **Proven**: it's ReScript's model, adjusted for Rust's tree of modules,
  cycles and visibility.

## Alternatives

- **One JS file per crate**, with paths folded into names (`math$add`):
  simpler, but a 10-file crate becomes one large file, and the output no
  longer mirrors the source.
- **Named imports** (`import { add } from "./math.js"`): reads well until two
  modules both have `add`, which then needs renaming at the import.

## Consequences

- The output is a set of files. The CLI still takes one `-o <file>`, for the
  root, and places the other modules relative to it.
- Runtime helpers such as `$div` are still per file
  ([0012](0012-panics-and-runtime-helpers.md)), so they're repeated across files.
- The playground shows only the root file for now.
- Multiple *crates* (Cargo dependencies) remain future work. Each would become
  its own folder of modules.
