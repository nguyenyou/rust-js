// A write through an element of an element changes the collection itself,
// not a copy of the row: arrays of arrays, a `Vec` of arrays, and a field
// of an array's element.
#[derive(Debug, Clone, Copy)]
struct Cell {
    value: i32,
}

fn main() {
    let mut grid = [[0, 0, 0], [0, 0, 0]];
    let row = grid[0];
    grid[0][1] = 5;
    grid[1][2] += 7;
    println!("{grid:?} {row:?}");

    let mut rows = vec![[0, 0], [0, 0]];
    rows[1][0] = 3;
    println!("{rows:?}");

    let mut cells = [[Cell { value: 1 }, Cell { value: 2 }]];
    cells[0][1].value *= 10;
    println!("{cells:?}");

    let i = 1;
    let mut deep = [[[0, 0], [0, 0]], [[0, 0], [0, 0]]];
    deep[i][0][i] = 9;
    println!("{deep:?} {}", deep[1][0][1]);
}
