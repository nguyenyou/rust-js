// Calls that look like expressions can require a box and copy-back. Neither
// may escape an unselected branch or run before the condition is evaluated.
fn bump(x: &mut i32) -> i32 {
    *x += 1;
    *x
}

fn pick(take: bool) -> i32 {
    let mut x = 0;
    let picked = if take { bump(&mut x) } else { 7 };
    x * 10 + picked
}

fn both(take: bool) -> i32 {
    let mut x = 0;
    let mut y = 10;
    let picked = if take { bump(&mut x) } else { bump(&mut y) };
    x * 1000 + y * 100 + picked
}

fn guard(value: Option<i32>) -> i32 {
    let mut calls = 0;
    let matched = matches!(value, Some(n) if bump(&mut calls) == n);
    calls * 10 + if matched { 1 } else { 0 }
}

fn nested(take: bool, inner: bool) -> i32 {
    let mut x = 0;
    let picked = if take { if inner { bump(&mut x) } else { 3 } } else { 7 };
    x * 10 + picked
}

fn main() {
    println!("{} {}", pick(false), pick(true));
    println!("{} {}", both(false), both(true));
    println!("{} {} {}", guard(None), guard(Some(1)), guard(Some(2)));
    println!("{} {} {}", nested(false, true), nested(true, false), nested(true, true));
    let mut x = 0;
    let picked = if bump(&mut x) == 1 {
        bump(&mut x)
    } else {
        bump(&mut x) + 10
    };
    println!("{} {}", x, picked);
    let mut total = 0;
    for i in 0..4 {
        let picked = if i % 2 == 0 { bump(&mut total) } else { 7 };
        println!("{} {}", total, picked);
    }
}
