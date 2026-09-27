// `place op= value`: Rust takes the value first, then the place, once each,
// though the place is read and written.
#[derive(Debug, Clone, Copy)]
struct P {
    x: i32,
}

fn main() {
    let mut log = Vec::new();
    let mut grid = [[1, 2], [3, 4]];
    let mut ps = vec![P { x: 1 }, P { x: 2 }];
    let mut arr = [P { x: 5 }, P { x: 6 }];
    let mut v = vec![10, 20, 30];
    {
        let mut at = |i: usize| {
            log.push(i);
            i
        };
        grid[at(1)][at(0)] += 10;
        grid[at(0)][at(1)] *= at(3) as i32;
        ps[at(1)].x += 5;
        arr[at(0)].x -= at(2) as i32;
        v[at(2)] += 1;
        v[1] += at(7) as i32;
    }
    println!("{grid:?} {ps:?} {arr:?} {v:?}");
    println!("{log:?}");
}
