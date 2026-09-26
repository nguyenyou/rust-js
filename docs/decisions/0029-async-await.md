# 0029. `async`/`.await` are JS's `async`/`await`; a future is a promise

Status: Accepted.

## Context

rust-js's own playground does its work through promises: `fetch`,
`WebAssembly.instantiate`, and timers. To write it in Rust, rust-js needs
async code. How the others do it (checked in local clones):

- **ReScript** has `async` and `await` keywords, and a `promise<'a>` type
  for JS promises. `let get = async key => await I.get(key)` compiles to
  `let get = async key => await I.get(key);`.
- **Scala.js** has `js.async { .. }` and `js.await(p)` on a
  `js.Promise[A]`. They compile to an async arrow and a JS `await`
  (`Closure` with the `async` flag, and `JSAwait`, in its IR).

In Rust, an `async fn` returns a *coroutine*, a state machine that does
nothing until something polls it. rustc writes `e.await` as a loop:

```text
match IntoFuture::into_future(e) {
    mut __awaitee => loop {
        match Future::poll(Pin::new_unchecked(&mut __awaitee), get_context(_task_context)) {
            Ready(result) => break result,
            Pending => {}
        }
        _task_context = yield ();
    }
}
```

Native Rust needs an executor (tokio, or `wasm-bindgen-futures` in the
browser) to drive that loop. JS has one built in: the event loop.

## Decision

**Async Rust is async JS, one to one.**

| Rust | JS |
|---|---|
| `async fn f(x: u32) -> u32 { .. }` | `async function f(x) { .. }` |
| `e.await` | `await e` |
| `async move { .. }` | `(async () => { .. })()` |
| `async \|y\| ..` | `async (y) => ..` |
| a future (`impl Future`, `dyn Future`, a `web::Promise<T>`) | a JS promise |
| `spawn(Box::new(async move { .. }))` | `(async () => { .. })();`, not awaited |

- **An `async fn`'s body is its coroutine's body.** rustc moves each
  parameter into the coroutine with `let x = x;`. In JS it's one function,
  so the inner `x` is the parameter itself. A pattern parameter (`(a, b)`)
  arrives as rustc's `__arg0`, named `param`, as in a plain `fn`.
- **`.await` is recognized whole**, like `for` (ADR 0025): the `match`
  on `IntoFuture::into_future(e)` whose one arm is the poll loop.
  `yield`, `poll` and the task context never reach the JS.
- **JS promises have a Rust type**, `web::Promise<T>`, which implements
  `Future` with `Output = T`, so rustc accepts `.await` on it. Its `poll` is
  never compiled: rust-js turns `.await` into `await`. The web crate's
  WebIDL promise results are now included (`HtmlImageElement::decode`,
  `Element::scroll_into_view`), and `extern` functions can return one:

  ```rust
  #[link_name = "node:timers/promises#setTimeout"]
  safe fn later(ms: u32, value: u32) -> Promise<u32>;
  ```
- **`web::spawn`** runs a future without waiting for it, for event handlers.
  It's an `extern` function of the `"this"` form (ADR 0024), so it's the
  promise itself.
- **A rejected promise throws at its `await`**, the way a panic does. In
  the playground, a panic in async code is reported like any other.

**One difference from Rust, chosen on purpose: a future starts when it's
made.** A Rust future does nothing until it's first polled. A JS promise runs
at once, up to its first `await`. So the code before a future's first
`.await` runs when the future is created, and a future that's never awaited
still runs. `test/async.rs` and the countdown example pin this down.

## Why

- **The JS reads like the Rust**, and like hand-written JS. ReScript and
  Scala.js do the same.
- **No runtime.** The event loop is the executor. There's no state machine,
  no waker and no polling in the output.
- **JS callers get what they expect**: an exported `async fn` returns a
  promise they can `await`.
- **The difference rarely shows** in UI code, which `.await`s what it
  starts. When it does show, what happens is what the JS says.

## Alternatives

- **Keep Rust's laziness**: a future becomes a function that starts the
  work (`() => promise`), and `.await` calls it. Every future would then need
  handling by its type (a JS promise or a Rust future), JS callers of an
  `async fn` would get a function, and the output would stop reading like
  JS.
- **Compile the state machine**, from MIR, with an executor in JS like
  `wasm-bindgen-futures`: faithful, but unreadable, and big.
- **Generators** (`function*` and a driver), as Babel did before ES2017:
  only worth it where JS has no `async`, and it has.

## Consequences

- `Promise` and `spawn` live in the web crate, though they're JS and not
  the web platform. A `js` crate may take them later.
- `extern` functions can't be generic, so a program declares `new Promise`
  for each result type it needs. The countdown example declares one for
  `Promise<()>`.
- Rust has no `#[test] async fn` (rustc rejects it), so tests can only see
  what runs before the first `.await`. The countdown's test checks that
  a click shows "3" at once.
- Not yet: `async` functions in traits, `IntoFuture` for your own types,
  streams, and joining several futures (`Promise.all`).
