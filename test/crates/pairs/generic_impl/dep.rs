//! A generic impl of the library's trait: its dictionary is made for each
//! set of dictionaries and drops it's given, which may be none.
pub trait Value {
    fn value(&self) -> u32;
}

pub struct Wrap<T>(pub T);

impl<T> Value for Wrap<T> {
    fn value(&self) -> u32 {
        7
    }
}

pub fn get<V: Value>(v: &V) -> u32 {
    v.value()
}
