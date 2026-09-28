// A `loop` is a value: what its `break` gives, from inside a loop in it
// too, by its label, or, for one that never ends, any type at all, as a
// function's result or an argument. From rustc's own tests.
fn conjure<T>() -> T {
    loop {}
}

fn pick(x: u8) -> u8 {
    x
}

fn main() {
    if false {
        let _never: u8 = conjure();
    }
    let mut n = 0;
    let doubled = loop {
        n += 1;
        if n == 3 {
            break n * 2;
        }
    };
    let found = 'outer: loop {
        for i in 0..100 {
            if i * i > 20 {
                break 'outer i;
            }
        }
    };
    let mut m = 0u8;
    let given = pick(loop {
        m += 1;
        if m > 2 {
            break m;
        }
    });
    println!("{doubled} {found} {given}");
}
