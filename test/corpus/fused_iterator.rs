// `fuse()` ends an iterator at its first `None`, as a JS iterator ends at
// its first `done`: an array's and a JS iterator chain's are fused as they
// are (ADRs 0036 and 0055). `impl FusedIterator` promises an iterator is,
// and has nothing to run.
use std::iter::FusedIterator;

struct Countdown(u32);

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            None
        } else {
            self.0 -= 1;
            Some(self.0)
        }
    }
}

impl FusedIterator for Countdown {}

// One that starts again after its `None`, which `fuse()` stops at.
struct Blinker(u32);

impl Iterator for Blinker {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        self.0 += 1;
        if self.0 % 3 == 0 { None } else { Some(self.0) }
    }
}

fn main() {
    println!("{}", Countdown(5).fuse().filter(|n| n % 2 == 0).count());
    let all: Vec<u32> = Countdown(4).fuse().map(|n| n * 10).collect();
    println!("{:?}", all);
    for n in Blinker(0).fuse() {
        println!("blink {}", n);
    }
    let lit: Vec<u32> = Blinker(3).fuse().collect();
    println!("{:?}", lit);
    let sizes = vec![1u32, 22, 333];
    println!("{}", sizes.iter().fuse().map(|s| s % 10).sum::<u32>());
    println!("{:?}", "héllo".chars().fuse().rev().collect::<String>());
}
