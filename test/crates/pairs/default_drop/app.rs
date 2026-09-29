//! A value of this crate's, with a destructor, taken by the library's default.
struct Resource {
    id: u32,
}

impl Drop for Resource {
    fn drop(&mut self) {
        println!("dropped {}", self.id);
    }
}

pub fn main() {
    dep::take(dep::Wrap(Resource { id: 7 }));
    println!("end");
}
