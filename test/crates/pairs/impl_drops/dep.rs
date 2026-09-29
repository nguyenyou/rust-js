//! A generic impl whose method takes its value, and drops what's in it:
//! called through the impl's dictionary, it's given the drop it needs.
pub trait Consume {
    fn consume(self) -> u32;
}

pub struct Wrap<T>(pub T);

impl<T: Clone> Consume for Wrap<T> {
    fn consume(self) -> u32 {
        1
    }
}

pub fn eat<C: Consume>(c: C) -> u32 {
    c.consume()
}
