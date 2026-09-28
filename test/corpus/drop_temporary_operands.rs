//@ run-fail: no third
// A value made before an operand that panics is dropped as the panic
// unwinds (ADR 0098): the call it was for never took it.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn make(name: &'static str) -> Noisy {
    println!("make {}", name);
    Noisy(name)
}

fn third(fail: bool) -> u32 {
    if fail {
        panic!("no third");
    }
    3
}

fn three(a: Noisy, b: Noisy, n: u32) -> u32 {
    println!("three {} {}", a.0, b.0);
    n
}

fn main() {
    println!("{}", three(make("a"), make("b"), third(false)));
    println!("{}", three(make("c"), make("d"), third(true)));
}
