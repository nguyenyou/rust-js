// std's iterator sources: `once` and `empty` are arrays, as std's other
// iterators are, and `repeat`, `successors` and `from_fn`, which can go on
// forever, are JS iterators, only taken from as far as they're used.

struct Counter(u32);

impl Iterator for Counter {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.0 < 3 {
            self.0 += 1;
            Some(self.0)
        } else {
            None
        }
    }
}

fn repeated<T: Clone>(first: T, n: usize) -> Vec<T> {
    std::iter::successors(Some(first), |item| Some(item.clone())).take(n).collect()
}

fn powers_of_two(limit: u32) -> Vec<u32> {
    std::iter::successors(Some(1u32), |&n| n.checked_mul(2)).take_while(|&n| n <= limit).collect()
}

fn countdown(from: u32) -> Vec<u32> {
    let mut n = from;
    std::iter::from_fn(|| {
        if n == 0 {
            None
        } else {
            n -= 1;
            Some(n + 1)
        }
    })
    .collect()
}

fn with_header(header: &str, lines: &[&str]) -> Vec<String> {
    std::iter::once(header).chain(lines.iter().copied()).map(|line| line.to_uppercase()).collect()
}

fn maybe_items(item: Option<i32>) -> Vec<i32> {
    item.into_iter().chain(item.iter().map(|n| n * 10)).collect()
}

// A `for` over an `Option` is one Rust warns of, but takes.
#[allow(for_loops_over_fallibles)]
fn main() {
    println!("{:?}", powers_of_two(100));
    println!("{:?} {:?}", countdown(4), countdown(0));
    println!("{:?}", with_header("title", &["a", "b"]));
    let nothing: Vec<i32> = std::iter::empty().collect();
    println!("{:?}", nothing);
    let dashes: String = std::iter::repeat('-').take(5).collect();
    let words: Vec<String> = std::iter::repeat(String::from("hi")).take(3).collect();
    println!("{} {:?}", dashes, words);
    let padded: Vec<i32> = [1, 2].into_iter().chain(std::iter::repeat(0)).take(5).collect();
    println!("{:?}", padded);
    println!("{:?} {:?}", maybe_items(Some(4)), maybe_items(None));
    let total: i32 = Some(3).iter().sum::<i32>() + None::<i32>.into_iter().sum::<i32>();
    println!("{}", total);
    let (left, right): (Vec<i32>, Vec<char>) = [(1, 'a'), (2, 'b')].into_iter().unzip();
    println!("{:?} {:?}", left, right);
    let squares: Vec<u64> = std::iter::successors(Some(2u64), |n| Some(n * n)).take(4).collect();
    println!("{:?}", squares);

    // The crate's own iterators, chained and zipped on, and stepped through.
    let chained: Vec<u32> = [10, 20].into_iter().chain(Counter(0)).collect();
    let zipped: Vec<(u32, char)> = Counter(0).zip("ab".chars()).collect();
    let labelled: Vec<(char, u32)> = "xyz".chars().zip(std::iter::repeat(7)).collect();
    println!("{:?} {:?} {:?}", chained, zipped, labelled);
    let mut fours = std::iter::repeat(4);
    let mut bigger = Counter(0).map(|n| n + 100);
    let (a, b) = (fours.next(), bigger.next());
    let rest: Vec<u32> = bigger.collect();
    println!("{:?} {:?} {:?}", a, b, rest);
    let short: Vec<u32> = Counter(0).skip_while(|&n| n < 2).collect();
    println!("{:?} {:?}", short, repeated(None::<i32>, 2));
    let mut rows: Vec<Vec<i32>> = std::iter::repeat(vec![0]).take(2).collect();
    rows[0].push(1);
    let mut k = 0;
    let squares: Vec<i32> = std::iter::repeat_with(|| {
        k += 1;
        k * k
    })
    .take(3)
    .collect();
    println!("{:?} {:?}", rows, squares);
    // `successors` finds the next item before it gives this one.
    let mut calls = 0;
    let taken = std::iter::successors(Some(1), |n| {
        calls += 1;
        Some(n + 1)
    })
    .take(3)
    .count();
    println!("{} {}", taken, calls);
    let maybe = Some("one");
    for item in &maybe {
        println!("{}", item);
    }
}
