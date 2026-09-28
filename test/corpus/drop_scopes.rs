// A destructor runs where Rust runs it (ADR 0098): a variable's at the end
// of its scope, in reverse order, however the scope ends; not a moved one's;
// an old value's when it's assigned over; a statement's value's at once.
// A struct's `drop` runs before its fields', which go in order.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Pair {
    first: Noisy,
    second: Noisy,
}

impl Drop for Pair {
    fn drop(&mut self) {
        println!("drop pair of {} and {}", self.first.0, self.second.0);
    }
}

// No `Drop` of its own: its fields' run, in order.
struct Holder {
    label: String,
    inner: Noisy,
    count: u32,
}

struct Unit;

impl Drop for Unit {
    fn drop(&mut self) {
        println!("drop unit");
    }
}

enum Shape {
    Empty,
    One(Noisy),
    Two { a: Noisy, b: Noisy },
}

fn consume(n: Noisy) {
    println!("consume {}", n.0);
}

fn early(stop: bool) -> u32 {
    let _a = Noisy("early a");
    if stop {
        return 1;
    }
    let _b = Noisy("early b");
    2
}

fn find(items: &[u32], wanted: u32) -> Option<usize> {
    let _guard = Noisy("find guard");
    let at = items.iter().position(|&x| x == wanted)?;
    Some(at + 1)
}

fn made(name: &'static str) -> Noisy {
    let n = Noisy(name);
    println!("made {}", n.0);
    n
}

fn main() {
    {
        let _x = Noisy("x");
        let _y = Noisy("y");
        {
            let _z = Noisy("z");
        }
        println!("inner done");
    }
    let moved = Noisy("moved");
    consume(moved);
    let maybe = Noisy("maybe");
    if early(true) == 1 {
        consume(maybe);
    }
    println!("{} {:?} {:?}", early(false), find(&[4, 5], 5), find(&[4], 5));
    let kept = made("kept");
    let mut slot = Noisy("old");
    slot = Noisy("new");
    println!("slot is {}", slot.0);
    std::mem::drop(kept);
    Noisy("statement");
    let _ = Noisy("ignored");
    let unit = Unit;
    drop(unit);
    for i in 0..3 {
        let _each = Noisy(if i == 1 { "one" } else { "other" });
        if i == 1 {
            break;
        }
    }
    let _pair = Pair { first: Noisy("first"), second: Noisy("second") };
    let _holder = Holder { label: String::from("h"), inner: Noisy("held"), count: 2 };
    let _list = vec![Noisy("v0"), Noisy("v1")];
    let _some = Some(Noisy("some"));
    let _none: Option<Noisy> = None;
    let _boxed = Box::new(Noisy("boxed"));
    let _tuple = (Noisy("t0"), 7, Noisy("t1"));
    let _shapes = [Shape::Empty, Shape::One(Noisy("one")), Shape::Two { a: Noisy("a"), b: Noisy("b") }];
    println!("end of main");
}
