//! The library's generic function as a value of one type.
pub fn main() {
    let f: fn(String) -> (String, String) = dep::duplicate::<String>;
    println!("{:?}", f("a".to_string()));
    let pairs: Vec<(u32, u32)> = [1u32, 2].into_iter().map(dep::duplicate).collect();
    println!("{:?}", pairs);
}
