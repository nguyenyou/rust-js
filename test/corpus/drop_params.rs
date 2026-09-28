// A parameter is its function's to drop however it's bound (ADR 0098): by
// value, by `ref`, or as `_`, it's dropped as the function ends.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn by_value(n: Noisy) {
    println!("by value {}", n.0);
}

fn by_ref(ref n: Noisy) {
    println!("by ref {}", n.0);
}

fn ignored(_: Noisy) {
    println!("ignored");
}

fn both(ref a: Noisy, _: Noisy, b: Noisy) {
    println!("both {} {}", a.0, b.0);
}

fn main() {
    by_value(Noisy("a"));
    by_ref(Noisy("b"));
    ignored(Noisy("c"));
    both(Noisy("d"), Noisy("e"), Noisy("f"));
    // A `let` that binds a new value by `ref` owns it too.
    let ref kept = Noisy("g");
    let same = Noisy("h");
    let ref borrowed = same;
    // `let _ = x` doesn't move `x`, so its scope drops it.
    let kept_too = Noisy("i");
    let _ = kept_too;
    println!("{} {}", kept.0, borrowed.0);
    println!("end");
}
