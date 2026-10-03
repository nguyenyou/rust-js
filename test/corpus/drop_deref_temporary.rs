// A method call through a `Box` a call returned dereferences it in place:
// the box is a temporary. One in an `if`'s condition is dropped before
// either branch (ADR 0098).
struct Temporary;

impl Drop for Temporary {
    fn drop(&mut self) {
        println!("drop");
    }
}

impl Temporary {
    fn ready(&self) -> bool {
        true
    }
}

fn make() -> Box<Temporary> {
    Box::new(Temporary)
}

fn main() {
    if make().ready() {
        println!("ready");
    }
}
