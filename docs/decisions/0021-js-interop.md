# 0021. JS interop: `extern` blocks name what JS has

Status: Accepted. The DOM itself comes generated, in the `web` crate: see
[0024](0024-web-crate.md), which also adds property and constructor forms.
Generic bindings use the tool attribute rejected below, which rust-js now
registers itself: see [0039](0039-generic-bindings.md).

## Context

A web program talks to the DOM. Rust needs a way to say "this function,
value or type exists in JS" and rust-js a way to compile a use of it.

ReScript spells this `external`, with attributes saying where the thing
lives: `@val` for a global, `@send` for a method call, `@module` for an
import. Rust already has a construct that means *"defined elsewhere, trust
this signature"*: the `extern` block. rustc type-checks calls to it and never
needs a body, so it works with rust-js unchanged, needs no proc macro
(the playground can't run those), and rust-analyzer understands it.

## Decision

JS things are declared in `unsafe extern "Rust"` blocks:

```rust
#![feature(extern_types)]

unsafe extern "Rust" {
    type Element;                                   // an opaque JS value
    safe static document: &'static Document;        // a global
    safe fn alert(message: &str);                   // a global function
    #[link_name = "createElement"]                  // the JS name
    safe fn create_element(this: &Document, tag: &str) -> &'static Element;
}
```

| Rust | JS |
|---|---|
| `type Element;` | an opaque JS value, held as `&'static Element` |
| `safe static document: ..` | the global `document` |
| `safe fn alert(..)`, called `alert(m)` | `alert(m)` |
| a first parameter named `this` | a method call: `create_element(document, "p")` is `document.createElement("p")` |
| `#[link_name = "createElement"]` | the JS name, if not the Rust one |
| a dotted `#[link_name = "console.log"]` | a path from a global: `console.log(m)` |

Values cross the boundary in their usual JS form (ADRs 0011, 0013, 0020,
0022, 0023), with no conversion.

Every module reserves the names of the globals the crate uses, so a local
can't hide one: a local named `console` becomes `console$1`.

## Why

- **It's plain Rust.** Nothing new to learn or to parse; rustc checks every
  use against the declared signature.
- **`"Rust"`, not `"C"`.** With the C ABI, rustc warns that `&str` and
  closures aren't FFI-safe. The Rust ABI means "passed as they are", which
  is what happens in JS.
- **`safe fn`** (Rust 2024) makes the calls usable without `unsafe` blocks
  in the program. The `unsafe extern` is the one place that says "I vouch
  for these signatures".
- **Extern types** are exactly "a value you can only hold a reference to":
  no size, no fields, and `&'static Element` is `Copy`, so a handle can be
  captured by several closures.
- **`this` for methods** keeps the DOM's names and argument order visible,
  and the JS reads like hand-written JS.

## Alternatives

- **A proc macro, like `wasm-bindgen`'s `#[wasm_bindgen]`**: more flexible
  syntax, but a macro crate to write and maintain, and proc macros can't run
  in the playground.
- **Tool attributes** (`#[rust_js::method]`): clearer than a parameter name,
  but they need `#![register_tool(rust_js)]` in every program.
- **A hand-written JS glue file per program**: works for anything, but every
  DOM call becomes two declarations.

## Consequences

- Programs need `#![feature(extern_types)]`, which is fine on the pinned
  nightly (ADR 0003) and in the playground.
- A signature is a promise: if JS returns `null` where Rust says
  `&'static Element`, nothing checks it. `Option` (with enums with fields)
  will make that expressible.
- Property access and `new` came later, as `#[link_name]` forms (`"get value"`,
  `"set value"`, `"new Event"`): see ADR 0024.
- Importing from a JS module came later, as `#[link_name = "module#path"]`:
  see ADR 0028. `#[link(name = "..")]` would be the obvious spelling, but
  rustc warns about it on a `"Rust"` block.
- Rust forbids a `let` named like a static, so the reserved names matter for
  global functions and paths: a local `console` becomes `console$1`, next to
  `console.log(..)`.
