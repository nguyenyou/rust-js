//@ compile-fail: rust-js does not support a temporary with a destructor yet
// A trait's default method, copied into each impl (ADR 0049), is checked
// for what it drops as any body is (ADR 0098): a temporary with a
// destructor, not dropped, would print one line fewer than Rust.
struct Noisy(u32);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

trait Greet {
    fn id(&self) -> u32;

    fn greet(&self) {
        if let Some(_) = &Some(Noisy(1)) {
            println!("hello");
        }
    }
}

struct Person;

impl Greet for Person {
    fn id(&self) -> u32 {
        7
    }
}

fn main() {
    Person.greet();
}
