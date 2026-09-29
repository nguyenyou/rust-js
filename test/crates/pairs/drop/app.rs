//! A value of it, made and dropped here: its destructor is the library's.
pub fn main() {
    {
        let _r = dep::make();
        println!("body");
    }
    let r = dep::make();
    std::mem::drop(r);
    println!("end");
}
