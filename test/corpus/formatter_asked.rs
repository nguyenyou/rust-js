// A writer that asks its `Formatter` for a width no placeholder gives it
// is told `None` (ADR 0143): it takes the options object all the same.
use std::fmt;

struct W;

impl fmt::Display for W {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {:?}", f.width(), f.align().is_none())
    }
}

fn main() {
    println!("{}", W);
}
