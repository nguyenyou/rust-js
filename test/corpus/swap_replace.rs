// `mem::swap` and `mem::replace` of places, and `hint::black_box`: while
// the call has a place's `&mut`, nothing else can use it, so writing each
// place in turn is exact. Neither drops what it moves out: that's moved
// to the other place, or returned.
use std::hint::black_box;
use std::mem;

struct Loud(&'static str);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Pair {
    left: Vec<i32>,
    right: Vec<i32>,
    label: String,
}

fn settle(done: &mut bool) -> bool {
    mem::replace(done, true)
}

fn main() {
    let (mut a, mut b) = (1, 2);
    mem::swap(&mut a, &mut b);
    println!("{a} {b}");

    let mut pair = Pair { left: vec![1, 2], right: vec![3], label: String::from("p") };
    mem::swap(&mut pair.left, &mut pair.right);
    let mut other = String::from("q");
    mem::swap(&mut pair.label, &mut other);
    println!("{:?} {:?} {} {}", pair.left, pair.right, pair.label, other);

    let old = mem::replace(&mut pair.left, vec![9]);
    println!("{:?} {:?}", old, pair.left);
    let _ = mem::replace(&mut a, 6);
    println!("{a}");

    let mut done = false;
    println!("{} {}", settle(&mut done), done);
    println!("{} {}", settle(&mut done), done);

    let mut loud = Loud("first");
    let first = mem::replace(&mut loud, Loud("second"));
    println!("replaced {}", first.0);
    let mut third = Loud("third");
    mem::swap(&mut loud, &mut third);
    println!("holding {} {}", loud.0, third.0);

    println!("{}", black_box(40) + black_box(2));
}
