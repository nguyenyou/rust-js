// `Some(..)` is any `Some`, as `Some(_)` is: its `..` names no fields, but
// it's still `Some`. From rustc's nonzero-enum.rs, where it was taken as
// `None`, the variant with no fields.
fn main() {
    let none: Option<u8> = None;
    let some = Some(3u8);
    if let Some(..) = none {
        println!("None matched Some(..)");
    }
    if let Some(..) = some {
        println!("Some matched Some(..)");
    }
    for value in [none, some] {
        let name = match value {
            Some(..) => "some",
            None => "none",
        };
        println!("{name} {}", matches!(value, Some(..)));
    }
}
