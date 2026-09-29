//! A trait's default method, which an impl of the library's keeps: it drops
//! what it's given, a consumer's value too.
pub trait Take: Sized {
    fn take(self) {}
}

pub struct Wrap<T>(pub T);

impl<T> Take for Wrap<T> {}

pub fn take<T: Take>(value: T) {
    value.take();
}
