# react: React for rust-js

Write React components in Rust. rust-js compiles them to the components
you'd write in JSX by hand ([ADR 0041](../docs/decisions/0041-react.md)):

```rust
#![allow(non_snake_case)]

use react::html::button;
use react::{Element, use_state};

pub fn App() -> Element {
    let (count, set_count) = use_state(0);
    button()
        .class_name("counter")
        .on_click(move |_| set_count.update(|count| count + 1))
        .children(("Count is ", count))
}
```

```jsx
// App.jsx
import { useState } from "react";

export function App() {
  const [count, setCount] = useState(0);
  return <button className="counter" onClick={() => setCount((count) => count + 1 | 0)}>Count is {count}</button>;
}
```

It's all bindings: rustc checks the types, and nothing of this crate ends up
in the JS.

| Rust | JSX |
|---|---|
| `div()`, `button()`, .. in `react::html` | `<div />`, `<button />` |
| `.class_name("a")`, `.html_for(id)`, `.r#type("button")` | `className="a"`, `htmlFor={id}`, `type="button"` |
| `.attr("aria-hidden", "true")` | `aria-hidden="true"` |
| `.on_click(move \|e\| ..)` | `onClick={(e) => ..}` |
| `.children(("Count is ", count))` | `Count is {count}` |
| `.key(t.id)` | `key={t.id}` |
| `component(Card, CardProps { title, children })` | `<Card title={title}>{children}</Card>` |
| `component(App, ())` | `<App />` |
| `fragment((a, b))`, `strict_mode(app)` | `<>{a}{b}</>`, `<StrictMode>..</StrictMode>` |

A component is a `pub fn` with a capitalized name that returns `Element`.
Its props, if it has any, are a struct, and a field named `children` holds
its children. A tuple of children is several of them, a `Vec` is a list
whose items need keys, and `None` renders nothing.

**Hooks** are React's, in snake case: `use_state`, `use_state_with`,
`use_reducer`, `use_effect`, `use_effect_on_every_render`,
`use_layout_effect`, `use_memo`, `use_callback`, `use_ref` and `use_id`.

- What a hook gives back is `&'static T`, read-only as React's state is, and
  `Copy`, so every handler can `move` it in.
- An effect's dependencies are a tuple: `(count, name)` is `[count, name]`,
  and `()` is `[]`.
- An effect's closure returns nothing, or the closure that cleans it up.

**React DOM**: `react::dom::create_root(element).render(app)`.

## Build it

```bash
react/build.sh -o target/libreact.rmeta     # also writes target/libweb.rmeta, which it uses
rust-js App.rs -- --extern react=target/libreact.rmeta -L target
```

`bun run build` does the first step. In a Vite project,
[vite-plugin-rust-js](../vite-plugin/index.js) does both, on every save. See
[examples/vite-react](../examples/vite-react/README.md).
