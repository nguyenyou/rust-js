// An auto trait's impl, as `unsafe impl Sync`, says what the type may do
// across threads, and has nothing to run: JS runs a module on one thread,
// so it changes nothing. It's how a `Cell` goes in a static.
use std::cell::Cell;
use std::marker::PhantomData;
use std::panic::{RefUnwindSafe, UnwindSafe};

struct Counter {
    hits: Cell<u32>,
}

unsafe impl Sync for Counter {}

static COUNTER: Counter = Counter { hits: Cell::new(0) };

struct Handle<T> {
    id: u32,
    marker: PhantomData<T>,
}

unsafe impl<T> Send for Handle<T> {}
impl<T> Unpin for Handle<T> {}
impl<T> UnwindSafe for Handle<T> {}
impl<T> RefUnwindSafe for Handle<T> {}

fn hit() -> u32 {
    COUNTER.hits.set(COUNTER.hits.get() + 1);
    COUNTER.hits.get()
}

fn shared<T: Sync>(value: &T) -> &T {
    value
}

fn main() {
    println!("{} {} {}", hit(), hit(), COUNTER.hits.get());
    let handle: Handle<String> = Handle { id: 7, marker: PhantomData };
    println!("{} {}", shared(&handle).id, shared(&3));
}
