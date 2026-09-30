# 0113. A plain rustc compiles a program too

Status: Accepted. Amends [0039](0039-generic-bindings.md),
[0040](0040-jsx.md), [0110](0110-stable-syntax.md) and
[0112](0112-rust-js-compiles-the-bindings.md).

## Context

rust-js compiles a program, and its binding crates, with rustc and rust-js's
own tool and syntax (ADRs 0110, 0112). A user's editor and `cargo check` run
a plain rustc, which knows neither: `#[rust_js::link_name]` names a tool it
doesn't know, and `jsx!` a macro no crate has. So rust-js's own checks were
the only ones a program had, and an editor showed errors in every file.

## Decision

**A plain stable rustc compiles a program, and the crates it uses:** it
type-checks all of it but what's inside JSX.

- **rust-js's attributes are `cfg_attr(rust_js, ..)`:**
  `#[cfg_attr(rust_js, rust_js::link_name = "sonner#toast")]`. rust-js sets
  `cfg(rust_js)`, for a program and, with `--rustc`, for a binding crate; a
  plain rustc leaves the attribute out. The crates' generators write it, and
  `js::import!` and `js::camel_case!` do.
- **`jsx!` is react's macro, which a file with JSX imports**, `use react::jsx;`,
  as any macro. To a plain rustc it's an `Element` it doesn't look inside,
  `Element::__jsx()`, a placeholder named so no prop is. To rust-js, which
  compiles JSX itself before names resolve, it's the Rust rust-js writes for
  it: rust-js rewrites the call's JSX, `jsx! { @rust_js .. }`, and the macro
  passes it on as it is. So the call stays, the import is used, and a missing
  one is the same error in both.

## Why

- **`cfg_attr` is plain stable Rust**, and the one way a crate can carry an
  attribute of a tool rustc doesn't know without an error.
- **A placeholder, not a second JSX compiler**: a proc macro could type-check
  what's inside JSX, but it would be another JSX implementation to agree with
  rust-js's.

## Consequences

- **The test `a plain stable rustc compiles the template's app and the crates
  it uses`**: js, webapi and react, and the vite-react app, with no
  `RUSTC_BOOTSTRAP`.
- **What a plain rustc doesn't check is inside JSX**, and a variable used only
  there is unused to it: a warning.
