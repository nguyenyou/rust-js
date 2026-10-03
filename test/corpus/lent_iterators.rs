// A generic iterator lent as `&mut`, `fn skip<I: Iterator>(it: &mut I)`, is
// the JS iterator the lender steps (ADR 0071): the lender's local knows
// where it is, and what the borrower takes, it no longer has. What the
// borrower stops early, a `take` or a `for` that breaks, the lender keeps
// stepping, a lazy chain too.
use std::iter::Peekable;

fn skip<I: Iterator<Item = u32>>(it: &mut I, n: usize) {
    for _ in 0..n {
        it.next();
    }
}

fn first(it: &mut impl Iterator<Item = u32>) -> Option<u32> {
    it.next()
}

fn number<I: Iterator<Item = char>>(it: &mut Peekable<I>) -> u32 {
    let mut n = 0;
    while let Some(c) = it.next_if(|c| c.is_ascii_digit()) {
        n = n * 10 + c.to_digit(10).unwrap();
    }
    n
}

fn word<I: Iterator<Item = char>>(it: &mut I) -> String {
    let mut s = String::new();
    while let Some(c) = it.next() {
        if c == ' ' {
            break;
        }
        s.push(c);
    }
    s
}

fn take2<I: Iterator<Item = u32>>(it: &mut I) -> Vec<u32> {
    it.take(2).collect()
}

fn until_zero<I: Iterator<Item = u32>>(it: &mut I) -> u32 {
    let mut sum = 0;
    for x in it {
        if x == 0 {
            break;
        }
        sum += x;
    }
    sum
}

struct Countdown(u32);

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            None
        } else {
            self.0 -= 1;
            Some(self.0)
        }
    }
}

fn main() {
    let mut it = vec![1, 2, 3, 4, 5].into_iter();
    skip(&mut it, 2);
    println!("{:?} {:?}", first(&mut it), it.collect::<Vec<_>>());
    let mut c = Countdown(5);
    skip(&mut c, 1);
    println!("{:?} {:?}", first(&mut c), c.next());
    let mut chars = "12+345".chars().peekable();
    let a = number(&mut chars);
    chars.next();
    println!("{} {}", a, number(&mut chars));
    let mut words = "hello big world".chars();
    let (x, y) = (word(&mut words), word(&mut words));
    println!("{} {} {}", x, y, word(&mut words));
    let mut lazy = std::iter::successors(Some(1u32), |n| Some(n * 2)).map(|n| n + 1);
    let a = take2(&mut lazy);
    println!("{:?} {:?}", a, lazy.next());
    let mut v = vec![1, 2, 0, 3, 4, 0, 5].into_iter();
    let s1 = until_zero(&mut v);
    let s2 = until_zero(&mut v);
    println!("{} {} {:?}", s1, s2, v.next());
}
