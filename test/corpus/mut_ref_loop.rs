// `for x in &mut v` of values that aren't objects is an index loop (ADR
// 0099): `*x` names `v[i]`. For a `Vec`, an array and a slice, and
// `iter_mut()` of one; `continue` and `break` as in any loop.
struct Holder<'a> {
    list: &'a mut Vec<i32>,
}

fn main() {
    let mut ints = vec![1, 2, 3];
    for i in &mut ints {
        *i += 22;
    }
    println!("{:?}", ints);

    let mut halves = [1.5f64, 2.5];
    for x in halves.iter_mut() {
        *x *= 2.0;
    }
    println!("{:?}", halves);

    let mut counts = vec![1u8, 2];
    for c in counts.iter_mut() {
        *c = c.wrapping_mul(200);
    }
    println!("{:?}", counts);

    let mut words = vec![String::from("a"), String::from("b")];
    for w in &mut words {
        w.push('!');
    }
    println!("{:?}", words);

    for x in ints[1..].iter_mut() {
        *x = 0;
    }
    println!("{:?}", ints);

    let mut ns = vec![1, 2, 3, 4];
    for x in &mut ns[1..3] {
        *x = -*x;
    }
    for x in ns[..1].iter_mut() {
        *x = 9;
    }
    println!("{:?}", ns);

    let mut v = vec![1, 2, 3, 4, 5];
    for n in &mut v {
        if *n % 2 == 0 {
            continue;
        }
        *n *= 10;
        if *n > 20 {
            break;
        }
    }
    println!("{v:?}");

    // The loop changes the collection it started with, whatever `cur`
    // holds after.
    let mut a = vec![1, 2];
    let mut b = vec![10, 20];
    let mut cur = &mut a;
    for x in cur.iter_mut() {
        cur = &mut b;
        *x += 1;
        break;
    }
    cur[0] += 5;
    println!("{a:?} {b:?}");

    // And if it's a field that holds the reference.
    let (mut a, mut b) = (vec![1, 2], vec![10, 20]);
    let mut h = Holder { list: &mut a };
    for x in h.list.iter_mut() {
        h.list = &mut b;
        *x += 1;
        break;
    }
    h.list[0] += 5;
    println!("{a:?} {b:?}");
}
