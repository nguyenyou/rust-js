//@ edition: 2015
//@ run-fail: the answer isn't 41
// Before edition 2021, `panic!` of one argument panics with it as it is,
// through `std::rt::begin_panic`, not as a format string: `{}` is two
// braces. With none, it's "explicit panic". From rustc's own tests, which
// panic so in 72 of them.
fn check(n: i32, expected: i32) {
    if n != expected {
        panic!("the answer isn't 41");
    }
}

fn never(n: i32) {
    if n < 0 {
        panic!();
    }
    if n > 100 {
        panic!("{}");
    }
}

fn main() {
    never(5);
    check(41, 41);
    println!("checked");
    check(42, 41);
}
