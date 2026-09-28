//@ compile-fail: rust-js does not support `std::iter::Iterator::collect` of a value with a destructor yet
// An adapter that skips an item drops it: `skip_while` here drops the
// first fragment before `kept` is printed. The JS would never drop it, so
// what takes an iterator whose destructors rust-js can't follow, a
// `vec::IntoIter` of them, is rejected (ADR 0098).
struct Fragment(u32);

impl Drop for Fragment {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let fragments = vec![Fragment(1), Fragment(2), Fragment(3)];
    let kept: Vec<Fragment> = fragments.into_iter().skip_while(|f| f.0 < 2).collect();
    println!("kept {}", kept.len());
}
