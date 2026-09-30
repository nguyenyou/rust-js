# 0111. A JS type is a struct of a `JsObject`

Status: Accepted. Amends [0021](0021-js-interop.md), [0024](0024-web-crate.md)
and [0110](0110-stable-syntax.md): a binding's JS type is stable Rust.

## Context

A binding names a JS type Rust only holds a reference to: CodeMirror's
`EditorView`, the DOM's `Element`. ADR 0021 declared one as an extern type,
`unsafe extern "Rust" { pub type EditorView; }`, which is a nightly feature,
and ADR 0024 declared the DOM's as a struct of a `PhantomData` of one, the js
crate's `JsObject`. So every JS type came down to an extern type, and the js
crate, and every program declaring its own, needed `extern_types`, a feature
a stable release refuses (ADR 0110).

## Decision

**A JS type is a struct of a `JsObject`**, as ADR 0024's are:
`pub struct EditorView(PhantomData<JsObject>);`, beside the `extern` block
that takes and gives it.

- **`JsObject` is a struct no one makes:** its one field is private, and a
  `PhantomData<*mut ()>`, so it's neither `Send` nor `Sync`. It's marked
  `#[rust_js::js_object]`, which rust-js reads, as the js crate's own `JsObject`,
  in another crate too, as a tool's attributes are kept in a crate's metadata.
- **A struct whose first field is a `PhantomData` of it is a JS object**, as
  one of an extern type is: rust-js holds it as the object, and never copies
  it. A program never has a `JsObject` itself, only a reference a binding
  gives, which is the object.
- **An extern type still is one**, where a nightly allows it.
- **`object::is` takes `?Sized`**, where it took `PointeeSized`, a nightly
  bound only an extern type needed.

## Why

- **It's ADR 0024's form, already the DOM's:** one way to declare a JS type,
  in plain stable Rust, which reads as what it is, a Rust name for a JS object.

## Consequences

- **The js crate used no nightly feature but `register_tool`**, for its own
  `#[rust_js::link_name]`s, which plain rustc built it with, and since ADR
  0112, none.
- **The playground's bindings are the struct form**, and its crate is stable
  Rust, compiled with no `RUSTC_BOOTSTRAP` (the test `the playground's own Rust
  is stable Rust`).
