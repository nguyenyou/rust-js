// An atomic is a `{ value }`, as a `Cell` is (ADR 0096): each operation is
// the plain one, since JS runs a module on one thread, and each wraps as its
// integer type does. Its orderings are evaluated, and mean nothing.
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU64, AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
static READY: AtomicBool = AtomicBool::new(false);
static BIG: AtomicU64 = AtomicU64::new(u64::MAX - 1);
static SMALL: AtomicI8 = AtomicI8::new(120);

mod counter {
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub static HITS: AtomicUsize = AtomicUsize::new(0);

    pub fn hit() -> usize {
        HITS.fetch_add(1, Ordering::Relaxed) + 1
    }
}

fn next_id() -> usize {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

fn order(seen: &mut Vec<&'static str>, name: &'static str) -> Ordering {
    seen.push(name);
    Ordering::SeqCst
}

fn main() {
    println!("{} {} {}", next_id(), next_id(), NEXT_ID.load(Ordering::SeqCst));
    READY.store(true, Ordering::Release);
    println!("{} {}", READY.load(Ordering::Acquire), READY.swap(false, Ordering::AcqRel));
    println!("{} {}", READY.fetch_or(true, Ordering::SeqCst), READY.fetch_and(false, Ordering::SeqCst));
    println!("{} {}", READY.fetch_xor(true, Ordering::SeqCst), READY.load(Ordering::SeqCst));
    println!("{} {}", BIG.fetch_add(3, Ordering::SeqCst), BIG.load(Ordering::SeqCst));
    println!("{} {}", SMALL.fetch_add(10, Ordering::SeqCst), SMALL.fetch_sub(-100, Ordering::SeqCst));
    println!("{}", SMALL.load(Ordering::SeqCst));
    println!("{} {}", NEXT_ID.fetch_max(10, Ordering::SeqCst), NEXT_ID.fetch_min(4, Ordering::SeqCst));
    println!("{} {}", NEXT_ID.fetch_xor(6, Ordering::SeqCst), NEXT_ID.load(Ordering::SeqCst));
    println!("{:?}", NEXT_ID.compare_exchange(2, 9, Ordering::SeqCst, Ordering::Relaxed));
    println!("{:?}", NEXT_ID.compare_exchange(3, 9, Ordering::SeqCst, Ordering::Relaxed));
    println!("{:?} {:?}", NEXT_ID.compare_exchange_weak(3, 1, Ordering::SeqCst, Ordering::Relaxed), NEXT_ID);
    println!("{} {} {}", counter::hit(), counter::hit(), counter::HITS.load(Ordering::SeqCst));

    let mut seen = Vec::new();
    let local = AtomicUsize::new(5);
    local.store(local.load(order(&mut seen, "load")) * 2, order(&mut seen, "store"));
    println!("{} {:?} {}", local.into_inner(), seen, AtomicUsize::default().load(Ordering::SeqCst));
}
