//! The app (ADR 0100): it uses `models`, and through it `validation`, as
//! rust-js compiles each on its own. Native Rust runs the same `main`.
use models::{Bag, Email, Point, Role, User};

struct Loud(&'static str);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

pub fn main() {
    let ada = User { role: Role::Admin, ..User::new("Ada", "ada@example.com", 36) };
    let kid = User { age: 9, ..ada.clone() };
    println!("{:?}", ada);
    println!("{:?} {:?}", ada.validate(), kid.validate());
    println!("{:?}", User::new("Bob", "nowhere", 40).validate());
    println!("{} {}", ada == kid, ada == ada.clone());
    println!("{} {}", matches!(ada.role, Role::Admin), validation::between(kid.age, 1, 10));

    // A clone is its own value, whoever changes it.
    let mut older = ada.clone();
    older.birthday();
    println!("{} {}", ada.age, older.age);

    // A clone of a library's type is its own, down to its `Vec`.
    let mut bag = Bag::default();
    let kept = bag.clone();
    bag.add(3);
    println!("{:?} {:?}", bag, kept);

    // A value copied out of a constant is the copy's.
    let mut p = models::origin();
    p.x = 5;
    println!("{:?} {:?} {:?}", p, models::origin(), Point { y: 2, ..p });

    // `==` is the impl's, not its fields'.
    println!("{}", Email("Ada@Example.com".to_string()) == Email("ada@example.com".to_string()));

    // A library's serde derives, and its errors, caught here.
    let json = serde_json::to_string(&ada).unwrap();
    println!("{json}");
    println!("{:?}", serde_json::from_str::<User>(&json).map(|user| user == ada));
    // Found by what `models` reads a field with, and what reads its text.
    for text in [r#"{"name": 5}"#, r#"{"name": "a", "email": "b", "age": -1, "role": "Admin"}"#] {
        match serde_json::from_str::<User>(text) {
            Ok(user) => println!("parsed {user:?}"),
            Err(e) => println!("error: {e}"),
        }
    }

    // A library's generic function drops what it's given, of a type of this crate's.
    println!("{}", models::count(vec![Loud("a"), Loud("b")]));
}
