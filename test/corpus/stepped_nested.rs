// A closure's body, and a trait's default body copied into an impl, find
// their own stepped iterators, as a function's does (ADR 0071): each is a
// body of its own, entered and left the same way.
trait Second {
    fn items(&self) -> Vec<u32>;

    fn second(&self) -> u32 {
        let items = self.items();
        let mut it = items.iter();
        it.next();
        *it.next().unwrap_or(&0)
    }
}

struct Numbers;

impl Second for Numbers {
    fn items(&self) -> Vec<u32> {
        vec![4, 5, 6]
    }
}

fn main() {
    let skip_one = |v: &Vec<u32>| {
        let mut it = v.iter();
        it.next();
        it.map(|x| x * 10).collect::<Vec<u32>>()
    };
    println!("{:?} {}", skip_one(&vec![1, 2, 3]), Numbers.second());
}
