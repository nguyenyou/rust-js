# web: the DOM for rust-js

The `web` crate declares the web platform for rust-js programs: DOM
bindings **generated from W3C's WebIDL**, the same source TypeScript's
`lib.dom.d.ts` and Rust's `web-sys` come from. It holds declarations only,
so it's never compiled to JS: a program calls what it declares, and the
calls become plain JS. See [ADR 0024](../docs/decisions/0024-web-crate.md).

```rust
use web::{document, element, event_target, node};

let b = document::create_element(document, "button");   // document.createElement("button")
node::set_text_content(b, "+");                          // b.textContent = "+"
event_target::add_event_listener(b, "click", Box::new(move |_| { .. }));
element::append(app, b);                                  // app.append(b)
```

- Each interface is a type (`Element`, `HtmlInputElement`) and a module of
  its members (`element`, `html_input_element`).
- Attributes are a getter and, if writable, a setter:
  `html_input_element::value(i)`, `html_input_element::set_value(i, "x")`.
- Inheritance is `Deref`: an `&HtmlButtonElement` goes wherever an
  `&Element` or `&Node` is expected.
- `unchecked_from` is a cast:
  `html_input_element::unchecked_from(document::create_element(document, "input"))`.
- Results that may be `null` say so in their docs, but aren't checked yet.
- A promise is a `Promise<T>`, to `.await`: `window::fetch_with_str(window, url).await`
  ([ADR 0029](../docs/decisions/0029-async-await.md)).
- Binary data is JS's `ArrayBuffer` and `Uint8Array`: `response::bytes(r).await`.
- An optional argument adds a form: `text_encoder::encode_with_input(e, "hi")`,
  `text_decoder::decode_with_uint8_array(d, bytes)`.
- A namespace is a module: `web_assembly::compile(bytes).await` is
  `await WebAssembly.compile(bytes)`. An `object` parameter takes any Rust value
  as `&dyn Any`, such as a struct for an import object.

## Use it

```bash
web/build.sh -o target/libweb.rmeta                              # its metadata, for the host
./target/debug/rust-js app.rs -- --extern web=target/libweb.rmeta
```

The playground compiles every program with the `web` crate available.

## Regenerate it

```bash
cd web && bun install && bun generate.ts
```

`generate.ts` holds every rule: which specs and interfaces, how WebIDL
types map to Rust, and the names. A member is generated only if rust-js
supports all its types; the run prints what it skipped, and why. To update
the platform, bump `@webref/idl` in `package.json` and regenerate. The diff
of `src/lib.rs` is what changed.
