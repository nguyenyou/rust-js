// A shared reference to a `static mut` is the value it points to, as any
// shared reference is (ADR 0023): a number's reference is its value then,
// and an object's the object (ADR 0096). Rust makes writing the static while
// the reference is used undefined, so no program it defines can tell.
#[derive(Debug, Clone, Copy)]
struct Stats {
    calls: u32,
    last: i64,
}

static mut COUNT: u32 = 0;
static mut STATS: Stats = Stats { calls: 0, last: -1 };
static mut NAME: &str = "start";

fn show(n: &u32) -> String {
    format!("<{}>", n)
}

#[allow(static_mut_refs)]
fn main() {
    unsafe {
        COUNT += 2;
        println!("{} {}", COUNT, show(&COUNT));
        let stats = &STATS;
        println!("{:?} {}", stats, stats.calls + 1);
        STATS.calls = 5;
        STATS.last = 9;
        println!("{} {} {}", STATS.calls, &STATS.last, STATS.calls.pow(2));
        NAME = "renamed";
        println!("{} {}", NAME, NAME.is_empty());
        let total = [&COUNT, &STATS.calls].iter().map(|n| **n).sum::<u32>();
        println!("{}", total);
    }
}
