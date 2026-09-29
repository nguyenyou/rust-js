//! A `Copy` enum with fields: each variant with them is an object (ADR 0033).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Circle(i32),
    Dot,
}

pub const SHAPE: Shape = Shape::Circle(1);

pub fn shape() -> Shape {
    SHAPE
}
