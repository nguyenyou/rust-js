//@ compile-fail: rust-js does not support constants of type `U` yet
// A union's constant, and its fields read, are rejected, not a crash: rust-js
// has no representation of a union yet. From rustc's union-const-codegen.rs.
union U {
    a: u64,
    b: u64,
}

const C: U = U { b: 10 };

fn main() {
    let (a, b) = unsafe { (C.a, C.b) };
    println!("{a} {b}");
}
