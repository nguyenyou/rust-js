// A user `Deref`, `DerefMut` or `IndexMut` is its method, called where
// rustc calls it: for `*w`, for a method or a field reached through `w`,
// and for `grid[i]` written to. What it returns is a reference, which is
// the value it points to (ADR 0023), or the object for a `&mut` (ADR 0025).
use std::ops::{Deref, DerefMut, Index, IndexMut};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn sum(&self) -> i32 {
        self.x + self.y
    }

    fn shift(&mut self, by: i32) {
        self.x += by;
        self.y += by;
    }
}

struct Tracked {
    point: Point,
    reads: std::cell::Cell<u32>,
}

impl Deref for Tracked {
    type Target = Point;

    fn deref(&self) -> &Point {
        self.reads.set(self.reads.get() + 1);
        &self.point
    }
}

impl DerefMut for Tracked {
    fn deref_mut(&mut self) -> &mut Point {
        &mut self.point
    }
}

// A chain: `Outer` to `Tracked` to `Point`.
struct Outer(Tracked);

impl Deref for Outer {
    type Target = Tracked;

    fn deref(&self) -> &Tracked {
        &self.0
    }
}

struct Grid {
    cells: Vec<Point>,
    width: usize,
}

impl Index<(usize, usize)> for Grid {
    type Output = Point;

    fn index(&self, (row, col): (usize, usize)) -> &Point {
        &self.cells[row * self.width + col]
    }
}

impl IndexMut<(usize, usize)> for Grid {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut Point {
        &mut self.cells[row * self.width + col]
    }
}

fn main() {
    let mut t = Tracked { point: Point { x: 1, y: 2 }, reads: std::cell::Cell::new(0) };
    println!("{} {} {:?}", t.sum(), t.x, *t);
    t.shift(10);
    t.y = 0;
    let copied = *t;
    t.x = -1;
    println!("{:?} {:?} {}", copied, t.point, t.reads.get());

    let outer = Outer(Tracked { point: Point { x: 4, y: 5 }, reads: std::cell::Cell::new(0) });
    println!("{} {} {}", outer.sum(), outer.x, outer.reads.get());

    let mut grid = Grid { cells: vec![Point { x: 0, y: 0 }; 4], width: 2 };
    grid[(1, 0)].x = 7;
    grid[(0, 1)].shift(3);
    println!("{:?} {:?} {:?}", grid[(1, 0)], grid[(0, 1)], grid.cells);
}
