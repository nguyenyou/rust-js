// A generic iterator kept in a variable that `next()` steps through is a JS
// iterator from where it's bound, `Iterator.from(it)`, which knows where it
// is (ADR 0071): an array's, a lazy one's or the crate's own, stepped, and
// what's left of it taken by `collect` or a `for` loop.
fn sum<I: Iterator<Item = u32>>(mut it: I) -> u32 {
    let mut total = 0;
    while let Some(x) = it.next() {
        total += x;
    }
    total
}

fn pairs<I: Iterator<Item = u32>>(it: I) -> Vec<(u32, u32)> {
    let mut it = it;
    let mut out = Vec::new();
    while let (Some(a), Some(b)) = (it.next(), it.next()) {
        out.push((a, b));
    }
    out
}

fn after_first<I: Iterator<Item = u32>>(mut it: I) -> (Option<u32>, Vec<u32>) {
    let first = it.next();
    let mut rest = Vec::new();
    for x in it {
        rest.push(x * 10);
    }
    (first, rest)
}

fn firsts<T, I: Iterator<Item = T>>(mut it: I) -> (Option<T>, Option<T>) {
    let a = it.next();
    let b = it.next();
    (a, b)
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
    let v = vec![1, 2, 3, 4, 5];
    println!("{} {} {}", sum(v.iter().copied()), sum(Countdown(4)), sum(std::iter::successors(Some(1u32), |n| (*n < 4).then(|| n + 1))));
    println!("{:?} {:?}", pairs(v.iter().copied()), pairs(Countdown(5)));
    println!("{:?} {:?}", after_first(v.into_iter()), after_first(Countdown(3)));
    println!("{:?}", firsts(vec![None, Some(2)].into_iter()));
    println!("{:?}", firsts(Vec::<()>::new().into_iter()));
}
