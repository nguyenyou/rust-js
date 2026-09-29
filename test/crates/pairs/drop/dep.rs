//! A library's type with a destructor of its own (ADR 0100).
pub struct Resource(pub u32);

impl Drop for Resource {
    fn drop(&mut self) {
        println!("dropped {}", self.0);
    }
}

pub fn make() -> Resource {
    Resource(1)
}
