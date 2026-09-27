//@ ignore-rust-js: array repeat expressions, `[x; N]`
// Each element is its own copy: changing one row leaves the others.
fn main() {
    let mut row = [0; 3];
    row[1] = 5;
    let mut grid = [[0; 3]; 2];
    grid[0][1] = 5;
    println!("{row:?} {grid:?}");
}
