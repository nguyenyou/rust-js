// A slice's pattern tests its length and its items, and an array's its
// items alone: `[first, .., last]`, `[1, n, rest @ ..]` (ADR 0123). What
// `..` binds is a copy of the items it stands for, as `&v[a..b]` is (ADR
// 0063).

fn describe(xs: &[i32]) -> String {
    match xs {
        [] => "empty".to_string(),
        [one] => format!("one: {}", one),
        [first, second] => format!("two: {} {}", first, second),
        [1, n, 3, rest @ ..] => format!("one, {}, three, then {:?}", n, rest),
        [first, middle @ .., last] => format!("{} .. {} around {:?}", first, last, middle),
    }
}

fn greet(words: &[&str]) -> String {
    match words {
        ["hello", name] => format!("hi {}", name),
        ["hello", names @ ..] => format!("hi all {}", names.len()),
        [.., "bye"] => "goodbye".to_string(),
        _ => "?".to_string(),
    }
}

// An array's parameter, taken apart where it's given.
fn sum3([x, y, z]: [i32; 3]) -> i32 {
    x + y + z
}

const ORIGIN: [i32; 2] = [0, 0];

fn main() {
    for xs in [&[][..], &[7][..], &[4, 5][..], &[1, 2, 3, 4, 5][..], &[9, 8, 7, 6][..]] {
        println!("{}", describe(xs));
    }
    for words in [&["hello", "ann"][..], &["hello", "a", "b"][..], &["so", "bye"][..], &["x"][..]] {
        println!("{}", greet(words));
    }

    // A range, an `Option` and a tuple inside.
    let xs = [5, 6, 7];
    if let [3..=14, ..] = xs {
        println!("starts small");
    }
    let found = [Some(1), None, Some(3)];
    if let [Some(a), None, Some(b)] = found {
        println!("{} {}", a, b);
    }
    let pairs = [(1, 'a'), (2, 'b')];
    let [(n, c), ..] = pairs;
    println!("{} {}", n, c);

    // An array's, irrefutable: its items, and its rest, a shorter array.
    let [a, b, c] = [10, 20, 30];
    let [first, rest @ ..] = [1, 2, 3, 4];
    let [.., last] = rest;
    println!("{} {} {} {} {:?} {}", a, b, c, first, rest, last);
    println!("{}", sum3([1, 2, 3]));

    // A constant array as a pattern: its items.
    for p in [[0, 0], [0, 1]] {
        match p {
            ORIGIN => println!("origin"),
            [x, y] => println!("{} {}", x, y),
        }
    }

    // Through a `&mut`: what's bound names the item, and writes it.
    let mut grid = [1, 2, 3];
    if let [head, .., tail] = &mut grid {
        *head += 10;
        *tail *= 2;
    }
    println!("{:?}", grid);
    let mut names = vec!["x".to_string(), "y".to_string()];
    if let [first, ..] = names.as_mut_slice() {
        first.push('!');
    }
    println!("{:?}", names);
}
