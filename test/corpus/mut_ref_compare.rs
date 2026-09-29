// A `&mut` compared, matched, shown or sorted is what it points at (ADR
// 0099), whatever the `&mut` is in JS: a place a variable names, a box or a
// handle, a temporary's box, and in a `Vec`, an `Option` or a struct too.
// Found in review: each compared or showed the handle itself.
#[derive(Debug)]
struct S<'a> {
    r: &'a mut i32,
}

fn main() {
    let mut a = 1;
    let x = &mut a;
    println!("{}", x == &mut 1);

    let (mut b, mut c) = (2, 2);
    let mut p = &mut b;
    let q = &mut c;
    println!("{} {} {}", p == q, *p < 3, p > q);
    *p += 1;
    println!("{}", p > q);
    p = &mut b;
    println!("{}", p == &mut 2);

    let mut n = 4;
    let s = S { r: &mut n };
    println!("{}", s.r == &mut 4);

    let mut m = 3;
    let mut r = &mut m;
    r = &mut *r;
    match r {
        &mut 3 => println!("three"),
        _ => println!("other"),
    }
    match r {
        v => *v += 1,
    }
    println!("{m}");

    let (mut i, mut j, mut k) = (3, 1, 2);
    let mut refs = vec![&mut i, &mut j, &mut k];
    refs.sort();
    println!("{refs:?} {} {:?}", refs.contains(&&mut 2), refs.iter().max());
    let mut z = 2;
    let zr = &mut z;
    println!("{}", refs.contains(&zr));
    let mut t = 7;
    let o = Some(&mut t);
    println!("{o:?} {}", o == Some(&mut 7));
    let mut u = 5;
    let held = S { r: &mut u };
    println!("{held:?}");
    let mut w = Some(5);
    if let Some(ref mut n) = w {
        *n += 1;
        println!("{n} {n:?}");
    }
}
