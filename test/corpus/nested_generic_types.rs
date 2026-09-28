// Types whose parts double at each level, as `S3<u8>` holds `S2<S2<u8>>`:
// what a type drops is found once for each type, not once for each path to
// it, or finding it takes exponential time (ADR 0098).
struct Noisy(u8);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct S0<T>(T, T);
struct S1<T>(Option<Box<S0<S0<T>>>>, Option<Box<S0<S0<T>>>>);
struct S2<T>(Option<Box<S1<S1<T>>>>, Option<Box<S1<S1<T>>>>);
struct S3<T>(Option<Box<S2<S2<T>>>>, Option<Box<S2<S2<T>>>>);
struct S4<T>(Option<Box<S3<S3<T>>>>, Option<Box<S3<S3<T>>>>, Option<T>);

fn main() {
    let deep = S4::<u8>(None, None, Some(7));
    let held = S4(None, None, Some(Noisy(1)));
    println!("{:?} {}", deep.2, held.2.is_some());
}
