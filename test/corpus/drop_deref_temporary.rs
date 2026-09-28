//@ compile-fail: rust-js does not support a temporary with a destructor yet
// A method call through a `Box` a call returned dereferences it in place:
// the box is a temporary, dropped at the end of its statement, which isn't
// supported yet (ADR 0098). Left out, its drop would never run.
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
