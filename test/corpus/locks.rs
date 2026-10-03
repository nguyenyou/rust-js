// A `Mutex` or an `RwLock` on one thread is a `RefCell` that can't be
// contested (ADR 0025): `{ value }`, whose `lock()` is always `Ok`, and
// `*m.lock().unwrap() += 1` is `m.value += 1`. An `Arc` is an `Rc`. A
// guard of a number held in a variable names the cell's `value`, as a
// `&mut` held in one names its place (ADR 0099).
use std::cell::RefCell;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
struct Stats {
    hits: u32,
    names: Vec<String>,
}

fn record(stats: &Mutex<Stats>, name: &str) {
    let mut s = stats.lock().unwrap();
    s.hits += 1;
    s.names.push(name.to_string());
}

fn total<T: Fn(u32) -> u32>(shared: Arc<Mutex<u32>>, f: T) -> u32 {
    f(*shared.lock().unwrap())
}

fn main() {
    let stats = Arc::new(Mutex::new(Stats { hits: 0, names: Vec::new() }));
    let other = Arc::clone(&stats);
    record(&stats, "a");
    record(&other, "b");
    let hits = stats.lock().unwrap().hits;
    println!("{} {:?}", hits, stats.lock().unwrap().names);

    let count = Arc::new(Mutex::new(5));
    *count.lock().unwrap() += 2;
    let seen = count.clone();
    *seen.lock().expect("not poisoned") *= 3;
    let now = *count.lock().unwrap();
    println!("{} {}", now, total(count, |n| n + 1));

    let config = RwLock::new(vec![1, 2]);
    config.write().unwrap().push(3);
    let len = config.read().unwrap().len();
    println!("{:?} {}", *config.read().unwrap(), len);

    let m = Mutex::new(String::from("x"));
    m.lock().unwrap().push('y');
    println!("{}", m.into_inner().unwrap());

    let counter = Mutex::new(0);
    {
        let mut num = counter.lock().unwrap();
        *num += 1;
        *num *= 10;
        println!("inside {}", *num);
    }
    println!("{:?}", *counter.lock().unwrap());
    let cell = RefCell::new(1.5);
    {
        let mut v = cell.borrow_mut();
        *v += 1.0;
        let r = *v;
        println!("{} {}", r, v.to_string());
    }
    let text = RwLock::new(String::from("a"));
    {
        let mut w = text.write().unwrap();
        w.push('b');
        *w += "c";
    }
    let slots = vec![Mutex::new(1), Mutex::new(2)];
    let mut i = 0;
    let mut first = slots[i].lock().unwrap();
    i += 1;
    *first += 10;
    println!("{} {}", *first, i);
    drop(first);
    println!("{:?}", slots.iter().map(|m| *m.lock().unwrap()).collect::<Vec<_>>());
    let r = text.read().unwrap();
    println!("{} {} {}", *r, r.len(), cell.borrow());
}
