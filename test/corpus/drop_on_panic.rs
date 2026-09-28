//@ run-fail: gave up at 3
// A panic drops what's live as it unwinds (ADR 0098): each scope's
// `finally`, inside out, and not what wasn't made yet or was moved.
struct Noisy(u32);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn keep(n: Noisy) -> Noisy {
    n
}

fn count(to: u32) {
    let _outer = Noisy(100);
    for i in 1..=to {
        let _step = Noisy(i);
        if i == 3 {
            panic!("gave up at {}", i);
        }
    }
    let _never = Noisy(200);
}

fn main() {
    let first = Noisy(1);
    let _kept = keep(first);
    count(5);
}
