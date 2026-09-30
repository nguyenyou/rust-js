# 0110. rust-js's syntax is stable Rust's

Status: Accepted: a program's syntax. A binding's JS type is a struct of a
`JsObject` since [0111](0111-js-types-as-structs.md). Amends [0039](0039-generic-bindings.md),
[0046](0046-camel-case-crates.md) and [0109](0109-stable-release.md).

## Context

rust-js checks a program with a stable release's rustc (ADR 0109), but its
own syntax was four of rustc's unstable features, which it turned on for
every crate it compiled: `register_tool`, for `#[rust_js::link_name]`;
`custom_inner_attributes`, for `#![rust_js::import]` and
`#![rust_js::camel_case]`; and `stmt_expr_attributes` and `decl_macro`, for
the code JSX expands to. A program could use them too: `let x = #[allow(unused)]
5;` compiled in rust-js, and not in the release it says it checks with.

## Decision

**A program is stable Rust: rust-js turns on no unstable feature for it**,
and rustc refuses one it asks for, `#![feature]`, as the release does, unless
`RUSTC_BOOTSTRAP=1`.

- **`rust_js` is a tool rustc knows**, as it knows `rustfmt` and `clippy`:
  rust-js adds it to rustc's `registered_tools`. So `#[rust_js::link_name = ".."]`
  is stable Rust, as `#[rustfmt::skip]` is, with no `register_tool`.
- **The code JSX expands to may use what it needs, as std's macros do:** its
  spans are of an expansion, `jsx`, that allows `stmt_expr_attributes` and
  `decl_macro`, and a component's props `macro` is
  `#[allow_internal_unstable(stmt_expr_attributes)]`, for its expansion's
  `#[rust_js::jsx]`, in another crate too. The expansion is transparent, so
  names resolve as they're written. What a program writes itself is its own.
- **An import is `js::import!("./App.css");`**, written where it's needed,
  at the crate root or in a module: stable Rust has no inner attribute of a
  tool. The `js` crate's macro writes a `const _` with `#[rust_js::import]`,
  which rust-js reads and writes nothing of.
- **A camelCase crate is `js::camel_case!();`**, at its root, the same way.
- **The expansions are hashed with the crate's `StableCrateId`**, which rustc
  computes once it has the crate's context, after JSX needs it: rust-js
  computes it with rustc's own functions, in rustc's order, and checks it's
  rustc's. It's rustc's invariant, for incremental builds: across crates,
  metadata names an expansion by its crate's number, not by its hash.

## Why

- **A program checked by 1.98.1 is 1.98.1's Rust**: what compiles in rust-js
  compiles in the user's own `cargo check`, but for rust-js's own tool.
- **Allowing unstable features by span, not by crate, is std's way**: `?`
  and `format!` use unstable features in stable programs, in their own code.

## Consequences

- **The tests** (`a program can use no unstable feature rust-js's syntax once
  used`): each feature rust-js turned on is refused, with the release's own
  error, which its rustc confirms, and a binding needs none; `js::import!` at
  the root, in a module, and written out as its attribute; and a component of
  another crate as a JSX tag, on a stable release.
- **A binding's JS type was still a nightly feature**, an extern type,
  `unsafe extern "Rust" { pub type EditorView; }`: ADR 0111 makes it a struct
  of a `JsObject`, stable Rust.
- **The binding crates still used `register_tool`** for their own attributes,
  which plain rustc built them with: ADR 0112 has rust-js compile them.
- **A plain rustc, a user's own `cargo check`, doesn't know the `rust_js`
  tool**: only rust-js does.
