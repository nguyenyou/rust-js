// A trait's `&mut self` method on a number or a `String`: the impl's
// method takes a box, as any `&mut` parameter to one does (ADR 0074), so
// a call of it, `n.bump()`, boxes `n` and takes it back after (ADR 0099).
trait Bump {
    fn bump(&mut self);
    fn bump_by(&mut self, by: &mut i32) -> i32;
}

impl Bump for i32 {
    fn bump(&mut self) {
        *self += 1;
    }
    fn bump_by(&mut self, by: &mut i32) -> i32 {
        *self += *by;
        *by += 1;
        *self
    }
}

trait Shout {
    fn shout(&mut self);
}

impl Shout for String {
    fn shout(&mut self) {
        self.push('!');
    }
}

fn main() {
    let mut n = 1;
    n.bump();
    n.bump();
    let mut step = 10;
    let total = n.bump_by(&mut step);
    println!("{n} {step} {total}");

    let mut name = String::from("hey");
    name.shout();
    Shout::shout(&mut name);
    println!("{name}");

    let mut counts = vec![1, 2];
    counts[1].bump();
    println!("{counts:?}");
}
