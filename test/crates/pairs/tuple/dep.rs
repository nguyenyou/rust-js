//! A tuple constant a function hands out: a JS array, like a struct's object.
pub const ORIGIN: (i32, i32) = (0, 0);

pub fn origin() -> (i32, i32) {
    ORIGIN
}

pub fn corners() -> [(i32, i32); 2] {
    [ORIGIN, (1, 1)]
}
