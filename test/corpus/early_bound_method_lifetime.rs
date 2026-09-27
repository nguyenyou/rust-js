// A trait method with a lifetime of its own that a `where` clause makes
// early-bound, `fn pick<'b>(..) where 'a: 'b`, called through a trait
// object and a generic. rust-js resolved it without that lifetime, and rustc
// failed. Found by rustc's `issue-26802`.
trait Picker<'a> {
    fn pick<'b>(&self, x: &'b u8) -> u8
    where
        'a: 'b,
    {
        *x + 7
    }
}

struct Seven;
impl Picker<'static> for Seven {}

struct Double;
impl Picker<'static> for Double {
    fn pick<'b>(&self, x: &'b u8) -> u8
    where
        'static: 'b,
    {
        *x * 2
    }
}

fn boxed(p: Box<dyn Picker<'static>>) -> u8 {
    p.pick(&4)
}

fn generic<P: Picker<'static>>(p: P) -> u8 {
    p.pick(&5)
}

fn main() {
    println!("{} {} {} {}", boxed(Box::new(Seven)), boxed(Box::new(Double)), generic(Seven), generic(Double));
}
