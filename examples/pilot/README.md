# The pilot: a full-stack contacts app

ROADMAP M3.3. One Cargo workspace, three crates, and a Vite app:

```
examples/pilot/
├── models      Contact, NewContact, Problem, and validate()  ◄── both sides
├── server      native Rust, std::net and serde_json:  /api/contacts
├── frontend    React, compiled by rust-js as Cargo checks it (ADR 0101)
└── web         Vite: imports `rust-js:frontend`, proxies /api to the server
```

The JS rust-js writes is committed beside the Rust it's from, as ReScript's
projects do (ADR 0041): [`frontend/src/api.js`](frontend/src/api.js) is
[`api.rs`](frontend/src/api.rs)'s, and `models/src/lib.js` the models'. A
change to the Rust is committed with its JS, which the test checks.

The client and the server share `models`: the JSON each sends is its
serde derives, and `validate` is the rule both hold a contact to. The form
checks before it sends; the server checks again, for requests that aren't
the form's, and knows what the form can't, that an email is taken.

```
browser ──► Vite ── /api ──► server ── models::validate
   │                            │
   └── frontend (rust-js) ──────┴── models (serde) ── the same JSON, both ways
```

## Run it

```bash
bun run build                              # in the repository root, once
cd examples/pilot
cargo +1.98.1 run -p server    # http://127.0.0.1:3000
cd web && bun run dev                      # in another terminal
```

What it does, each checked by [`test/pilot.test.ts`](../../test/pilot.test.ts)
in a browser, with the server running:

- **Routing:** `#/`, `#/contacts/3` and `#/new`, followed on `hashchange`.
- **A list, searched as you type.** Each search aborts the one before, so a
  slow answer to an older search never replaces a newer one's.
- **Loading and errors:** each page says it's loading, and why it has
  nothing: a contact that doesn't exist, or a server that can't answer,
  with a button to try again.
- **A form**, validated as the server validates, every field at once, and
  then the server's errors by field: `ada@example.com is taken`.
- **An npm component:** [Sonner](https://sonner.emilkowal.ski)'s `<Toaster>`
  and `toast()`, bound in [`frontend/src/sonner.rs`](frontend/src/sonner.rs).
- **The server refuses** JSON that isn't a contact (400) and a contact that
  breaks the rules (422), with its reasons.

## What it found

Fixed in the compiler, each with a test and a mutation:

- **A binding used as a value** (`.map(abort_signal::aborted)`) wasn't
  supported, and neither was **a package's component** as a JSX tag, which
  lowers to one (ADRs 0039, 0040).

Fixed in the binding crates, each with a test:

- **The web crate had no options objects**, no `fetch` with a
  `RequestInit`, no `addEventListener` with a signal, and nothing had
  `encodeURIComponent`: the pilot declared each itself. Now they're the
  `webapi` and `js` crates', named as ReScript's (ADR 0102), and the pilot
  declares no bindings but Sonner's.

Worked around here, for M3.4:

- **Iterating `str::bytes()`** isn't supported; the pilot doesn't need it
  now that it has `js::encode_uri_component`.
- **`Result::as_ref`** isn't supported: the form uses `.ok()`.
- **The bindings are a path into this repository** (`../../../react`) until
  the resources are published (M4); an app outside it would depend on its
  installed `@rust-js/resources/react`.
