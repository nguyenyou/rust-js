// Destructuring assignment, and the `let`s of patterns without a value it
// gives values to (ADR 0134): each variable of `let (a, b);` is a `let a;`
// of its own.

struct Point {
    x: i32,
    y: i32,
}

fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a
}

fn main() {
    let (a, b);
    (a, b) = (1, 2);
    let (mut c, [d, mut e]);
    (c, [d, e]) = (3, [4, 5]);
    c += 1;
    e *= 2;
    let Point { x, y };
    Point { x, y } = Point { x: 7, y: 8 };
    println!("{} {} {} {} {} {} {}", a, b, c, d, e, x, y);

    let mut slots = [0; 4];
    [slots[0], _, .., slots[3]] = [9, 10, 11, 12];
    let (first, last);
    [first, .., last] = [1, 2, 3, 4];
    println!("{:?} {} {} {}", slots, first, last, fibonacci(10));
}
