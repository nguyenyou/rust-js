// An iterator chain runs each item through every stage before the next, as
// Rust's does (ADR 0139): a stage whose closure does what can be seen makes
// the chain lazy from there, and one that stops early stops it.
fn noisy(tag: &str, x: i32) -> i32 {
    println!("{} {}", tag, x);
    x
}

// A chain kept in a variable that's returned: who ends it isn't known here.
fn doubled(v: &[i32]) -> impl Iterator<Item = i32> + '_ {
    let it = v.iter().map(|&x| noisy("returned", x * 2));
    it
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

    // A chain kept in a variable that's only iterated, and the other side of
    // a `zip` or a `chain`, run in their turn too.
    let kept = v.iter().map(|&x| noisy("kept", x));
    for x in kept {
        println!("kept body {}", x);
    }
    let counted = v.iter().filter(|&&x| noisy("counted", x) > 1);
    println!("{}", counted.count());
    let pairs: Vec<(i32, i32)> = v[..2]
        .iter()
        .map(|&x| noisy("left", x))
        .zip(v.iter().map(|&x| noisy("right", x)))
        .collect();
    println!("{:?}", pairs);
    let short: Vec<(i32, i32)> = v[..2].iter().copied().zip(v.iter().map(|&x| noisy("other", x))).collect();
    println!("{:?}", short);
    let back: Vec<i32> = doubled(&v).collect();
    println!("{:?}", back);
    let joined: Vec<i32> = v[..1].iter().copied().chain(v.iter().map(|&x| noisy("joined", x))).take(2).collect();
    println!("{:?}", joined);

    // Closures that do nothing that can be seen keep arrays.
    let doubled: Vec<i32> = v.iter().map(|x| x * 2).filter(|x| x % 4 == 0).take(1).collect();
    println!("{:?}", doubled);
}
