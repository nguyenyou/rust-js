// A copy is its own value: changing it, or the original, leaves the other
// as it was, however deep the value is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone)]
struct Shape {
    name: String,
    points: Vec<Point>,
}

fn moved(mut p: Point) -> Point {
    p.x += 100;
    p
}

fn main() {
    let a = Point { x: 1, y: 2 };
    let mut b = a;
    b.x = 10;
    println!("{a:?} {b:?}");

    let mut grid = [[0, 0, 0], [0, 0, 0]];
    let row = grid[0];
    grid[0][1] = 5;
    println!("{grid:?} {row:?}");

    let shape = Shape { name: "tri".to_string(), points: vec![a, b] };
    let mut copy = shape.clone();
    copy.name.push_str("-copy");
    copy.points[0].y = -1;
    copy.points.push(Point { x: 0, y: 0 });
    println!("{shape:?}\n{copy:?}");

    let c = moved(a);
    println!("{a:?} {c:?} {}", a == Point { x: 1, y: 2 });

    let mut points = vec![a; 3];
    for p in points.iter_mut() {
        p.y *= 3;
    }
    let first = points[0];
    points[0].x = 42;
    println!("{points:?} {first:?}");

    let mut nested = vec![vec![1, 2], vec![3]];
    let snapshot = nested.clone();
    nested[0].push(9);
    nested[1][0] = 0;
    println!("{nested:?} {snapshot:?}");
}
