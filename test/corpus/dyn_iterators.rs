// An iterator trait object, `Box<dyn Iterator>` or `&mut dyn Iterator`, is
// a JS iterator: an array becomes one, `v.values()`, and one already, a
// lazy chain or a `$iter`, is itself. A local lent as one, `&mut it`, is
// stepped through by what it's lent to, so it's a `$iter`, whose place the
// lender sees.
struct Countdown(u32);

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            None
        } else {
            self.0 -= 1;
            Some(self.0 + 1)
        }
    }
}

fn evens(v: &[u32]) -> Box<dyn Iterator<Item = u32> + '_> {
    Box::new(v.iter().copied().filter(|n| n % 2 == 0))
}

fn numbers(desc: bool) -> Box<dyn Iterator<Item = u32>> {
    if desc {
        Box::new(Countdown(3))
    } else {
        Box::new(vec![1, 2, 3].into_iter())
    }
}

fn total(it: &mut dyn Iterator<Item = u32>) -> u32 {
    let mut sum = 0;
    for n in it {
        sum += n;
    }
    sum
}

fn first_two(it: &mut dyn Iterator<Item = u32>) -> (Option<u32>, Option<u32>) {
    (it.next(), it.next())
}

fn doubled(it: &mut dyn Iterator<Item = u32>) -> Vec<u32> {
    it.map(|n| n * 2).collect()
}

fn main() {
    let v = vec![1, 2, 3, 4, 5, 6];
    println!("{:?}", evens(&v).collect::<Vec<_>>());
    println!("{:?}", evens(&v).map(|n| n * 10).max());
    println!("{:?}", numbers(true).collect::<Vec<_>>());
    println!("{}", numbers(false).sum::<u32>());

    let mut it = v.iter().copied();
    println!("{:?}", first_two(&mut it));
    println!("{}", total(&mut it));
    let mut rest = v.iter().copied();
    rest.next();
    println!("{:?}", doubled(&mut rest));

    let mut boxed: Box<dyn Iterator<Item = u32>> = Box::new(v.clone().into_iter());
    println!("{:?}", boxed.next());
    for n in boxed {
        print!("{n} ");
    }
    println!();

    let noisy: Box<dyn Iterator<Item = u32>> = Box::new(v.iter().map(|n| {
        println!("see {n}");
        *n
    }));
    println!("{:?}", noisy.take(2).collect::<Vec<_>>());
}
