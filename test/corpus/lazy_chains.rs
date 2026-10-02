// An iterator chain runs each item through every stage before the next, as
// Rust's does (ADR 0139): a stage whose closure does what can be seen makes
// the chain lazy from there, and one that stops early stops it.
fn noisy(tag: &str, x: i32) -> i32 {
    println!("{} {}", tag, x);
    x
}

fn main() {
    let v = vec![1, 2, 3, 4];

    let kept: Vec<i32> = v.iter().map(|&x| noisy("map", x * 2)).filter(|&x| noisy("filter", x) > 2).collect();
    println!("{:?}", kept);

    let first: Vec<i32> = v.iter().map(|&x| noisy("take", x)).take(2).collect();
    println!("{:?}", first);

    println!("{:?}", v.iter().map(|&x| noisy("find", x)).find(|&x| x > 1));
    println!("{}", v.iter().map(|&x| noisy("any", x)).any(|x| x == 2));
    println!("{:?}", v.iter().map(|&x| noisy("position", x)).position(|x| x == 3));
    println!("{:?}", v.iter().find_map(|&x| if noisy("find_map", x) > 1 { Some(x * 10) } else { None }));

    for x in v.iter().map(|&x| noisy("loop", x)).take_while(|&x| x < 3) {
        println!("body {}", x);
    }
    for (i, x) in v.iter().filter(|&&x| noisy("odd", x) % 2 == 1).enumerate() {
        println!("{} {}", i, x);
    }

    let total: i32 = v.iter().map(|&x| noisy("sum", x)).sum();
    let count = v.iter().inspect(|x| println!("saw {}", x)).filter(|&&x| x > 2).count();
    println!("{} {}", total, count);

    // Closures that do nothing that can be seen keep arrays.
    let doubled: Vec<i32> = v.iter().map(|x| x * 2).filter(|x| x % 4 == 0).take(1).collect();
    println!("{:?}", doubled);
}
