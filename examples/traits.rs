pub trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> String {
        "shape".to_string()
    } // a default method
}
pub trait Labeled: Shape {
    // a supertrait
    fn label(&self) -> String {
        format!("{} of area {}", self.name(), self.area())
    }
}

pub struct Circle {
    pub r: f64,
}
pub struct Square(pub f64);
pub enum Blob {
    Dot,
    Line(f64),
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        3.14 * self.r * self.r
    }
    fn name(&self) -> String {
        "circle".to_string()
    }
}
impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}
impl Shape for Blob {
    fn area(&self) -> f64 {
        match self {
            Blob::Dot => 0.0,
            Blob::Line(_) => 0.0,
        }
    }
}
impl Shape for f64 {
    fn area(&self) -> f64 {
        *self
    }
} // a primitive
impl<T: Shape> Shape for Vec<T> {
    fn area(&self) -> f64 {
        total(self)
    }
} // a generic impl
impl Labeled for Circle {}

pub fn total<T: Shape>(shapes: &[T]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}
pub fn largest(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).fold(0.0, f64::max)
}
pub fn fresh<T: Default + Shape>() -> f64 {
    T::default().area()
} // no receiver to dispatch on

pub fn demo() -> f64 {
    let c = Circle { r: 1.0 };
    let direct = c.area(); // the type is known here
    let mixed: Vec<Box<dyn Shape>> = vec![Box::new(c), Box::new(Square(2.0)), Box::new(3.0)];
    direct + total(&[Square(1.0), Square(2.0)]) + largest(&mixed) + vec![Square(1.0)].area()
}
