# 0142. A channel on one thread is a queue its ends share

Status: Accepted. Extends [0098](0098-destructors.md).

## Context

`std::sync::mpsc::channel()` is how Rust code hands values from producers
to a consumer. On one thread, which is all JS has, it's a queue, but not
only that: whether it's still connected is part of what it does.

```rust
let (tx, rx) = mpsc::channel();
produce(tx);                         // sends, then drops `tx`
while let Ok(job) = rx.recv() { .. } // ends once the queue is empty and
                                     // every sender is gone
```

`recv()` of an empty channel is an `Err` once every sender is dropped, and
`send` is an `Err` once the receiver is. A queue alone can't tell either.

## Decision

**A channel is a JS object its two ends share,** `{ queue, senders,
receiving }`, and each end is that object. The ends are values with
destructors (ADR 0098), which keep the count:

| Rust | JS |
|---|---|
| `let (tx, rx) = mpsc::channel();` | `$channel()`, `[channel, channel]` |
| `tx.send(item)` | `$send(tx, item)`: `Ok`, or `Err(SendError(item))` with no receiver |
| `rx.recv()` | `$recv(rx)`: the first item, or `Err(RecvError)` with no senders |
| `rx.try_recv()` | `$tryRecv(rx)`: or `Err(Empty)` or `Err(Disconnected)` |
| `tx.clone()` | `$cloneSender(tx)`: one sender more |
| a sender dropped | `$dropSender(tx)`: one sender fewer |
| the receiver dropped | `$dropReceiver(rx)`: none receives |

- **`recv()` of an empty channel whose senders are there panics,** with a
  message that says so. Rust's waits for another thread to send; on one
  thread none can, and it would wait forever.
- **`{:?}` of its errors is std's:** `RecvError`, `Empty`,
  `Disconnected` and `SendError { .. }`.
- **A channel of a value with a destructor is an error:** the queue would
  drop what's still in it, its own way.

## Why

- **The program's own loop ends as Rust's does.** A loop over `recv()`
  stops when the last sender is dropped, which is what a destructor is for.
- **One object for both ends** is all the state there is; a sender's
  identity doesn't matter, only how many there are.

## Alternatives

- **A queue, and no ends.** `recv()` couldn't tell an empty channel from a
  closed one: the loop above would never end, or end too soon.
- **Waiting for another send.** JS can't block, and nothing else can send
  while it waits.

## Consequences

- `sync_channel`, `recv_timeout`, `iter()` and `try_iter()` are still errors.
- `std::thread::spawn` is an error, so both ends are always on the one thread.
