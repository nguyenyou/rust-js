// A `&mut` to a number given where a generic `T` goes is a box, as a
// `&mut` to one is anywhere (ADR 0074): its impl's methods, taking it by
// value, change the number. From rustc's `issue-55809.rs`, which stopped
// when a generic `&mut T` became a box (ADR 0099) and a `T` wasn't one.
trait Bump {
    fn bump(self);
}

impl Bump for &mut i32 {
    fn bump(self) {
        *self += 1;
    }
}

fn go<T: Bump>(t: T) {
    t.bump();
}

// Passed on from a `&mut` parameter, a box already: the box.
fn via(x: &mut i32) {
    go(&mut *x);
    go(x);
}

fn pass<T>(_t: T) -> u8 {
    7
}

fn main() {
    let mut x = 1;
    go(&mut x);
    go(&mut x);
    via(&mut x);
    let mut unit = ();
    println!("{x} {} {}", pass(&mut x), pass(&mut unit));
}
