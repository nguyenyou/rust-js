// A trait object of a higher-ranked trait, `dyn for<'a> AsStr<'a, 'a>`: its
// dictionary is the impl's, whatever the lifetimes, which JS doesn't have.
// From rustc's any-lifetime-escape-higher-rank.rs.
trait AsStr<'a, 'b> {
    fn get(&'a self) -> &'b str;
}

impl<'a> AsStr<'a, 'a> for String {
    fn get(&'a self) -> &'a str {
        self
    }
}

fn show(s: &dyn for<'a> AsStr<'a, 'a>) -> String {
    s.get().to_uppercase()
}

fn main() {
    let b: Box<dyn for<'a> AsStr<'a, 'a>> = Box::new(String::from("bar"));
    println!("{} {}", b.get(), show(&String::from("four")));
}
