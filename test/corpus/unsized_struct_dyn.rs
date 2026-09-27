//@ compile-fail: whose last field is a `dyn`
// A struct whose last field becomes a trait object, `&Fat<dyn Show>`, has no
// JS shape yet: rejected, not run with a field that isn't one.
trait Show {
    fn show(&self) -> i32;
}

struct Seven;

impl Show for Seven {
    fn show(&self) -> i32 {
        7
    }
}

struct Fat<T: ?Sized> {
    size: i32,
    inner: T,
}

fn read(fat: &Fat<dyn Show>) -> i32 {
    fat.size + fat.inner.show()
}

fn main() {
    let fat = Fat { size: 1, inner: Seven };
    println!("{}", read(&fat));
}
