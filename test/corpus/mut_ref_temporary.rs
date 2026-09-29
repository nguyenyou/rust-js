// A `&mut` to a temporary, `&mut 1` or `&mut Some(3)`, has a home as long
// as the reference lives, as Rust gives it one (ADR 0099): a `let` of it
// in a variable, a box as a value, and a matched place. A `&mut` place a
// `match` or an `if let` takes apart is that place, as a `&` one is.
fn bump(x: &mut i32) {
    *x += 1;
}

fn main() {
    let x = &mut 1;
    *x += 1;
    bump(x);
    println!("{x}");

    let y: &mut i32 = &mut 42;
    *y *= 2;
    println!("{}", *y);

    bump(&mut 5);

    let mut opt = Some(1);
    if let Some(v) = &mut opt {
        *v += 10;
    }
    let mut words: Option<String> = Some(String::from("a"));
    if let Some(w) = &mut words {
        w.push('b');
    }
    println!("{opt:?} {words:?}");

    if let Some(v) = &mut Some(3) {
        *v += 1;
        println!("{v}");
    }
    match &mut (1, 2) {
        (a, b) => {
            *a += 10;
            *b += 20;
            println!("{a} {b}");
        }
    }
}
