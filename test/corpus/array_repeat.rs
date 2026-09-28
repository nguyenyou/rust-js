// `[x; N]`: `x` runs once, and the array is `N` copies of it, each its own,
// so changing one changes no other, as in `[[0; 3]; 2]`. Even `[f(); 0]`
// runs `f`. From rustc's own tests, which build arrays so in 46 of them.
#[derive(Clone, Copy, Debug)]
struct P {
    x: i32,
}

const SIZE: usize = 2;

fn made(name: &str, n: i32) -> i32 {
    println!("made {name}");
    n
}

fn main() {
    let zeros = [0u8; 1024];
    let flags = [true; 3];
    println!("{} {} {:?}", zeros.len(), zeros.iter().map(|&b| b as u32).sum::<u32>(), flags);
    let mut grid = [[0; 3]; SIZE * 2];
    grid[1][2] = 7;
    println!("{:?}", grid);
    let mut points = [P { x: 1 }; 3];
    points[0].x = 5;
    println!("{:?}", points);
    let once = [made("once", 4); 3];
    let none = [made("none", 9); 0];
    println!("{:?} {}", once, none.len());
    let options = [Some(2u8); 2];
    println!("{:?}", options);
}
