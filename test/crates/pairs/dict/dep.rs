//! A trait of the library's, an impl of it, and a generic function over it.
pub trait Value {
    fn value(&self) -> u32;

    fn twice(&self) -> u32 {
        self.value() * 2
    }
}

pub struct Seven;

impl Value for Seven {
    fn value(&self) -> u32 {
        7
    }
}

pub fn get<T: Value>(x: &T) -> u32 {
    x.value() + x.twice()
}
