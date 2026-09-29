//! A generic function, whose dictionary a function value of it must bind.
pub fn duplicate<T: Clone>(x: T) -> (T, T) {
    (x.clone(), x)
}
