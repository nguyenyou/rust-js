// A temporary with a destructor is dropped where rustc's scope tree ends it
// (ADR 0098): at the end of its statement, or, kept by a `let`, at the end
// of the block. A value made before an operand that may panic is moved by
// the call or the aggregate it's in, once every operand is made.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

impl Noisy {
    fn name(&self) -> &'static str {
        self.0
    }
}

fn make(name: &'static str) -> Noisy {
    println!("make {}", name);
    Noisy(name)
}

fn two(a: Noisy, b: Noisy) -> &'static str {
    println!("two {} {}", a.0, b.0);
    b.0
}

fn all(items: [Noisy; 2]) -> usize {
    println!("all {}", items[0].0);
    items.len()
}

fn main() {
    println!("{}", make("a").name());
    let first = make("b").0;
    let empty = make("c").name().is_empty();
    println!("{} {}", first, empty);
    {
        let kept = &make("d");
        println!("kept {}", kept.0);
        let field = &make("e").0;
        println!("field {}", field);
    }
    let last = two(make("f"), make("g"));
    println!("{} {}", last, all([make("h"), make("i")]));
    println!("end");
}
