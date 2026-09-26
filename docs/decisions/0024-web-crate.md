# 0024. The `web` crate: DOM bindings generated from WebIDL

Status: Accepted. Extends [0021](0021-js-interop.md).

## Context

ADR 0021 lets a program declare the JS it uses. A real web program uses a
lot of it, and writing every binding by hand doesn't scale. How the others
do it (checked in the local clones):

- **Scala.js** has scala-js-dom: hand-written facades.
- **ReScript** has two layers. Its runtime's `Dom` module declares only
  *type names* (295 lines, no functions), so libraries can agree on
  `Dom.element`. The functions come from a separate package,
  `@rescript/webapi` (rescript-lang/experimental-rescript-webapi), about
  26,000 lines *generated* with Microsoft's TypeScript-DOM-lib-generator.
  Properties are mutable record fields, so `el.textContent = ..` is free.
- **Rust's own `web-sys`** is generated from WebIDL, but it's built on
  wasm-bindgen's macros and its ABI, so it can't be used here.

The source all of these come from is the web platform's **WebIDL**. W3C
publishes it for every spec as `@webref/idl` (MIT).

Three things were missing in rust-js:

1. **Inheritance.** An `HTMLButtonElement` is an `Element` is a `Node`: code
   that takes a `Node` must accept all of them.
2. **Properties** (`el.textContent`, `input.value`) and constructors
   (`new Event(..)`).
3. **Another crate.** rust-js compiled one crate. Bindings belong in one of
   their own, compiled once.

## Decision

A crate named **`web`**, in `web/`, **generated** by `web/generate.ts` from
W3C's WebIDL (a pinned `@webref/idl`) into `web/src/lib.rs`. It holds
**declarations only**. rust-js never compiles it to JS: a program calls what
it declares, and rustc reads it as ordinary crate metadata (`libweb.rmeta`).

```rust
use web::{document, element, event_target, node};

let b = document::create_element(document, "button");   // document.createElement("button")
node::set_text_content(b, "+");                          // b.textContent = "+"
event_target::add_event_listener(b, "click", Box::new(move |_| ..));
element::append(app, b);                                  // app.append(b)
```

**Types.** Each interface is a zero-sized struct, and inheritance is `Deref`:

```rust
pub struct Element(PhantomData<JsObject>);         // `JsObject`: an extern type
impl Deref for Element { type Target = Node; .. }  // an Element is a Node
```

rust-js treats a struct whose only field is `PhantomData` of an extern type
as a **JS object**, and `Deref` on one as the object itself. Rust's deref
coercion does the rest: a `&HtmlButtonElement` goes wherever a `&Node` is
expected, with nothing written at the call site and nothing in the JS.

**Members.** One module per interface (`element`, `html_input_element`),
holding an `unsafe extern "Rust"` block (ADR 0021). A first parameter named
`this` makes a method, and `#[link_name]` says which JS form a call takes:

| `#[link_name]` | JS | generated for |
|---|---|---|
| `"append"` | `this.append(x)` | operations |
| `"get textContent"` | `this.textContent` | attributes |
| `"set textContent"` | `this.textContent = v` | writable attributes |
| `"new Event"` | `new Event(t)` | constructors |
| `"this"` | `this`, unchanged | `unchecked_from`: a cast |

Mixins (`Element includes ParentNode`) are copied into every interface that
includes them. `document` and `window` are globals at the crate root.

A **namespace** is a module of functions with no type, and each one calls a
path from a global: `web_assembly::compile(bytes)` is
`WebAssembly.compile(bytes)`. An interface in a namespace
(`[LegacyNamespace=WebAssembly] interface Module`) is named with it, as
`WebAssemblyModule` in `web_assembly_module`, and constructed as
`new WebAssembly.Module(..)`.

**Types across the boundary.**

| WebIDL | parameter | result |
|---|---|---|
| `DOMString`, `USVString`, `CSSOMString`, enums | `&str` | `String` |
| `boolean` | `bool` | `bool` |
| `byte` … `unsigned long` | `i8` … `u32` | same |
| `double`, `unrestricted double` | `f64` | `f64` |
| an interface in the crate | `&T` | `&'static T` |
| `ArrayBuffer`, `Uint8Array` (JS's own) | `&T` | `&'static T` |
| `Promise<T>` | `Promise<T>` | `Promise<T>` |
| `EventListener` | `Box<dyn FnMut(&Event)>` | – |
| `object` | `&dyn Any`: any Rust value, a struct say | `&'static JsObject` |
| a dictionary | – | a struct with its fields |
| `undefined` | – | `()` |

A member is generated only if all its types are in this table. So far that
leaves out `long long`, `float`, `any`, sequences, dictionaries as
parameters, and most callbacks. As rust-js grows, rerunning
the generator picks more up: promise results came with ADR 0029, as
`Promise<T>`.

JS's own types that WebIDL uses (`Promise`, `ArrayBuffer`, `Uint8Array`)
aren't in any WebIDL file. They're declared by hand at the top of the crate,
with the few members programs need so far: `uint8_array::new(buffer)`,
`uint8_array::length`, `array_buffer::byte_length`. Also:

- **Nullable:** a parameter takes the non-null type. A result is an
  `Option` (ADR 0030): `get_element_by_id(..) -> Option<&'static Element>`.
- **Optional** arguments: the shortest form keeps the name, and each
  optional argument, in order, adds a form, as web-sys names them:
  `encode(this)`, `encode_with_input(this, input)`; later ones add
  `_and_<name>` (`new_with_x_and_y`). An optional union gives a form per
  member, named by its type: `decode_with_array_buffer`,
  `decode_with_uint8_array`. The first optional argument whose type isn't
  supported (a dictionary, say) ends the forms.
  **Variadic** ones take a single value.
- **Unions:** one function per supported member. The first keeps the name
  (`append(this, &Node)`), the others add `_with_<type>`
  (`append_with_str(this, &str)`). A typedef of a union counts too:
  `fetch(this, &Request)` and `fetch_with_str(this, &str)` come from
  `RequestInfo`, and a union inside a union counts as its members
  (`BufferSource` includes `ArrayBufferView`, which includes `Uint8Array`).
- **Overloads:** the first keeps the name. A later one is named, as web-sys
  does, after the required arguments that set it apart from the first: by
  name where the first has none there (`set_range_text_with_start_and_end`,
  `alert_with_message`), by type where the types differ
  (`instantiate_with_web_assembly_module`).
- **Dictionaries** a function returns are Rust structs, which rust-js makes
  plain objects (ADR 0020), so their fields are read as they are:
  `source.instance`. Only those whose fields are all required, supported,
  and named the same in Rust; for now that's `WebAssemblyInstantiatedSource`.
- **Names:** snake_case of the IDL names, and web-sys-style type names
  (`HTMLInputElement` is `HtmlInputElement`). A Rust keyword gets a `_`.

**Which interfaces:** a list in `generate.ts`, the everyday DOM, grown as
programs need more. The Fetch Standard's `Request`, `Response` and
`Headers` came with async code (ADR 0029), for `window::fetch`, and the
Encoding Standard's `TextEncoder` and `TextDecoder` with binary data, and the
WebAssembly JS API (`WebAssembly`, `Module`, `Instance`, `Memory`) and its
Web API (`compile_streaming`), and tables (`HTMLTableElement` and its rows
and cells) for the playground (ADR 0032). It isn't
the whole platform (334 specs).

**Building:** `rustc --emit=metadata` produces `libweb.rmeta`, once per
target: the host for the tests, `wasm32-unknown-unknown` for the playground,
which always passes `--extern web=..`.

## Why

- **Generated means complete, correct, and easy to update.** It's how
  ReScript and web-sys got to full coverage.
- **Declarations only keep the JS clean.** No wrappers: `node::set_text_content(b, "+")`
  is `b.textContent = "+"`, as a person would write it.
- **Plain crate metadata:** rustc and rust-analyzer understand the crate,
  it's compiled once rather than with every program, and it needs no new
  cross-crate machinery in rust-js, because extern items already work
  across crates.

## Alternatives

- **Extern types for the DOM types (as in ADR 0021):** rejected by rustc.
  Under the new sized hierarchy an extern type isn't `MetaSized`, which
  `Deref::Target` requires, so there'd be no inheritance.
- **Hand-written bindings,** like scala-js-dom: never complete, and they
  drift from the platform.
- **Generating from TypeScript's `lib.dom.d.ts`,** as `@rescript/webapi`
  does: its types are TypeScript's, one step removed from the spec.
- **Methods (`app.append(b)`):** they need `impl` blocks, which means
  function bodies in another crate, and rust-js can only lower the
  current crate's bodies. The module-qualified call is the price for now.
- **Compiling the bindings into every program as a module:** it would make
  methods possible, but then every program type-checks the whole DOM.

## Consequences

- Calls read `element::append(app, b)`, not `app.append(b)`.
- `unchecked_from` is exactly that: `html_input_element::unchecked_from(e)`
  doesn't check that `e` is an input, just as a cast in TypeScript wouldn't.
- ADR 0021's extern types still work for one-off bindings.
- Updating means bumping `@webref/idl` and rerunning the generator. The
  crate's diff shows what changed on the platform.
