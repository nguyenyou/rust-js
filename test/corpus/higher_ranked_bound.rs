// A higher-ranked bound, `for<'a> T: Named<'a>`, is one dictionary a
// generic function passes on: lifetimes aren't in the JS. rust-js asked
// rustc about it with `'a` still bound, and rustc panicked. Found by rustc's
// `static-outlives-a-where-clause`.
trait Named<'a> {
    fn name(&self) -> String;
}

impl<'a, T: std::fmt::Debug> Named<'a> for T
where
    'static: 'a,
{
    fn name(&self) -> String {
        format!("{self:?}")
    }
}

trait Shout {
    fn shout(&self) -> String;
}

impl<T> Shout for T
where
    for<'a> T: Named<'a>,
{
    fn shout(&self) -> String {
        self.name().to_uppercase()
    }
}

fn loud<T: Shout>(value: T) -> String {
    value.shout()
}

fn main() {
    println!("{} {}", loud("quiet"), loud(Some(3)));
}
