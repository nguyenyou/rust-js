// A temporary with a destructor, taken apart by a pattern (ADR 0131): what
// the pattern moves out is its bindings', dropped as their scope ends, and
// the rest is dropped as the temporary's scope ends, at the end of its `let`,
// `match` or `if`, as Rust drops it.

#[allow(dead_code)]
struct Loud(u8);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

#[allow(dead_code)]
struct Pair {
    left: Loud,
    right: Loud,
}

#[allow(dead_code)]
enum Two {
    Both(Loud, Loud),
    One(Loud),
    Zero,
}

fn make(n: u8) -> Loud {
    println!("make {}", n);
    Loud(n)
}

fn pick(n: u8) -> Two {
    match n {
        0 => Two::Zero,
        1 => Two::One(make(10)),
        _ => {
            let (a, b) = (make(20), make(21));
            Two::Both(a, b)
        }
    }
}

// A parameter taken apart: the function owns what its pattern leaves.
fn some_borrowed((a, ref b): (Loud, Loud)) {
    println!("some {} {}", a.0, b.0);
}

fn all_borrowed((ref a, ref b): (Loud, Loud)) {
    println!("borrowed {} {}", a.0, b.0);
}

fn all_moved((a, b): (Loud, Loud)) {
    println!("moved {} {}", a.0, b.0);
}

fn main() {
    let (x, _) = (make(3), make(4));
    println!("after let {}", x.0);
    let Pair { right, .. } = Pair { left: make(30), right: make(31) };
    println!("after pair {}", right.0);

    for n in 0..3 {
        match pick(n) {
            Two::Both(a, _) => println!("both {}", a.0),
            Two::One(_) => println!("one"),
            Two::Zero => println!("zero"),
        }
        println!("after match {}", n);
    }

    let y = match (make(5), make(6)) {
        (p, _) => {
            println!("arm");
            p
        }
    };
    println!("after y {}", y.0);

    if let (Some(z), _) = (Some(make(7)), make(8)) {
        println!("if {}", z.0);
    }
    {
        // `ref b` keeps the temporary for the block, less what `a` moves out.
        let before = make(1);
        let (a, ref b) = (make(40), make(41));
        let (ref c, _) = (make(42), make(43));
        println!("in {} {} {} {}", before.0, a.0, b.0, c.0);
    }
    let whole = match make(9) {
        kept => kept.0,
    };
    some_borrowed((make(50), make(51)));
    all_borrowed((make(52), make(53)));
    all_moved((make(54), make(55)));
    let closure = |(a, ref b): (Loud, Loud)| println!("closure {} {}", a.0, b.0);
    closure((make(56), make(57)));
    println!("end {}", whole);
}
