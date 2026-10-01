// A range is a value as any struct is (ADR 0129): kept in a variable or a
// field, passed and returned, iterated where it's used, and asked whether
// it contains a number.

use std::ops::{Range, RangeInclusive};

struct Window {
    rows: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Labelled {
    span: Range<i32>,
    name: String,
}

fn sum_all<I: Iterator<Item = u32>>(items: I) -> u32 {
    items.sum()
}

// A range given where any `T` goes is the range, not its items.
fn shown_twice<T: Clone + std::fmt::Debug>(value: T) -> String {
    format!("{:?} {:?}", value.clone(), value)
}

fn total(r: Range<u32>) -> u32 {
    r.sum()
}

fn evens(r: RangeInclusive<i32>) -> Vec<i32> {
    r.filter(|n| n % 2 == 0).collect()
}

fn clamp_to(r: &Range<i32>, x: i32) -> i32 {
    if r.contains(&x) { x } else { r.start }
}

fn first_square_over(limit: u32) -> u32 {
    for n in 1.. {
        if n * n > limit {
            return n;
        }
    }
    unreachable!()
}

fn main() {
    let r = 2..7;
    let v: Vec<i32> = r.clone().collect();
    println!("{:?} {:?} {} {} {} {}", r, v, r.len(), r.is_empty(), (5..5).is_empty(), (5..2).len());

    let mut steps = 0..4;
    let first = steps.next();
    let last = steps.next_back();
    println!("{:?} {:?} {:?} {}", first, last, steps, steps.len());

    let inclusive = 1..=6;
    println!("{:?} {} {} {:?}", inclusive, inclusive.start(), inclusive.end(), evens(inclusive.clone()));
    let (low, high) = (3..=9).into_inner();
    println!("{} {} {}", low, high, (2..=1).is_empty());

    println!("{} {}", total(1..5), (0..5).into_iter().map(|x| x * 2).sum::<i32>());
    let ids: Vec<u32> = (1..).take(3).collect();
    let numbered: Vec<(usize, char)> = (1..).zip("abc".chars()).collect();
    println!("{:?} {:?} {}", ids, numbered, first_square_over(50));

    for x in [3, 7, 10] {
        println!(
            "{} {} {} {} {}",
            (1..4).contains(&x),
            (1..=7).contains(&x),
            (5..).contains(&x),
            (..8).contains(&x),
            (..=7).contains(&x)
        );
    }
    println!("{} {}", clamp_to(&(0..5), 9), clamp_to(&(0..5), 3));
    let unit = 0.0..1.0;
    println!("{} {} {:?}", unit.contains(&0.5), unit.contains(&f64::NAN), unit);

    let w = Window { rows: 1..3 };
    let letters = ["a", "b", "c", "d"];
    println!("{:?}", &letters[w.rows.clone()]);
    for row in w.rows {
        print!("{} ", row);
    }
    let span = 10..=12;
    for n in span {
        print!("{} ", n);
    }
    println!();

    println!("{:?} {:?} {:?} {:?} {:?}", 4.., ..4, ..=4, .., (-2i64)..=2);
    println!("{} {} {}", (1..4) == (1..4), (1..=4) != (1..=5), (2..) == (2..));
    let big: u64 = (1u64..4).sum();
    println!("{}", big);

    // A clone of a range stepped through is its own.
    let mut a = 0..3;
    a.next();
    let b = a.clone();
    a.next();
    let mut ids = 100..;
    let (first_id, second_id) = (ids.next(), ids.next());
    println!("{:?} {:?} {:?} {:?} {:?} {}", a, b, first_id, second_id, ids, (0..5).count());

    let labelled = Labelled { span: 2..5, name: "x".to_string() };
    let copy = labelled.clone();
    let spans = vec![0..2, 3..5];
    let lens: Vec<usize> = spans.iter().map(|s| s.len()).collect();
    println!("{:?} {} {:?} {:?}", labelled, labelled == copy, spans, lens);
    println!("{} {} {}", sum_all(1..4), sum_all(r_of(3)), sum_all((1..).take(3)));
    println!("{} {}", shown_twice(1..4), shown_twice(..=2));

    let mut numbers = vec![1, 2, 3, 4, 5];
    let cut = 1..3;
    let gone: Vec<i32> = numbers.drain(cut).collect();
    println!("{:?} {:?} {:?}", gone, numbers, &numbers[..=1]);

    // `char`s step past the surrogates, which no `char` is.
    let letters: String = ('a'..'e').collect();
    let around: Vec<char> = ('\u{d7fe}'..='\u{e001}').collect();
    for c in 'x'..='z' {
        print!("{}", c);
    }
    println!(" {} {:?} {}", letters, around, ("a".."m").contains(&"hat"));
}

fn r_of(n: u32) -> Range<u32> {
    0..n
}
