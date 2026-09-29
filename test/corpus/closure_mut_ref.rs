// A `&mut` to a closure is the closure (ADR 0099): calling a JS function
// changes what it captured, as calling it through the `&mut` does in
// Rust. For a type parameter bound by `FnMut`, an `impl FnMut`, a
// `dyn FnMut`, and a function, `&mut square`.
fn call<F: FnMut()>(f: &mut F) {
    f();
    (*f)();
}

fn passed_on<F: FnMut()>(f: &mut F) {
    call(f);
}

fn apply(f: &mut impl FnMut(i32) -> i32, x: i32) -> i32 {
    f(x) + f(x)
}

fn dynamic(f: &mut dyn FnMut() -> i32) -> i32 {
    f() * 10 + f()
}

fn square(x: i32) -> i32 {
    x * x
}

fn main() {
    let mut count = 0;
    let mut tick = || count += 1;
    call(&mut tick);
    passed_on(&mut tick);
    let r = &mut tick;
    r();
    println!("{count}");

    let mut total = 0;
    println!("{}", apply(&mut |x| {
        total += x;
        total
    }, 3));
    println!("{}", apply(&mut square, 4));

    let mut n = 0;
    println!("{}", dynamic(&mut || {
        n += 1;
        n
    }));
    println!("{total} {n}");
}
