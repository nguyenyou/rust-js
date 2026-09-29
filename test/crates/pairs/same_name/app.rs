//! Its `main` calls the library's: found in review, the app's was the one
//! renamed, so JS calling `main` found none.

pub fn main() {
    dep::main();
    println!("app's main");
}
