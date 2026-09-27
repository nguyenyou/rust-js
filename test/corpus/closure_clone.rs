//@ compile-fail: cloning a closure that changes what it captured
// A clone of a closure holds a copy of what it captured by value; a JS
// function shares it, so a clone that could tell is rejected.
#[derive(Clone)]
struct S(i32);

fn main() {
    let mut a = S(5);
    let mut hello = move || {
        a.0 += 1;
        a.0
    };
    let mut hello2 = hello.clone();
    println!("{} {}", hello2(), hello());
}
