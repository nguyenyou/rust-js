//@ run-fail: no second
// A variable moved into a call moves as the call's made, after every
// operand (ADR 0098): one that panics first leaves it owned, and its
// scope drops it as the panic unwinds.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn pair(a: Noisy, b: u32) -> u32 {
    println!("pair {} {}", a.0, b);
    b
}

fn second(fail: bool) -> u32 {
    if fail {
        panic!("no second");
    }
    2
}

fn main() {
    let first = Noisy("first");
    println!("{}", pair(first, second(false)));
    let again = Noisy("again");
    println!("{}", pair(again, second(true)));
}
