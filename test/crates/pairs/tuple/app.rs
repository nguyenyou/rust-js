//! Changing what it returned changes this crate's copy, not the constant.
pub fn main() {
    let mut p = dep::origin();
    p.0 = 5;
    let mut c = dep::corners();
    c[0].1 = 7;
    println!("{} {} {:?} {:?}", p.0, dep::origin().0, c, dep::corners());
}
