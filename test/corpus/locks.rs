// A `Mutex` or an `RwLock` on one thread is a `RefCell` that can't be
// contested (ADR 0025): `{ value }`, whose `lock()` is always `Ok`, and
// `*m.lock().unwrap() += 1` is `m.value += 1`. An `Arc` is an `Rc`.
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
}
