// Small std forms (ADR 0132): a `Box` from its value, `Default` of a `&str`
// or an array, rustc's name for a type, and writing to standard output and
// error, which never fails.

use std::io::{self, Write};

fn greet(err: &mut io::Stderr) -> io::Result<()> {
    writeln!(err, "to stderr {}", 1)?;
    Ok(())
}

fn table(rows: u32) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for row in 0..rows {
        write!(out, "{} ", row * row)?;
    }
    writeln!(out)?;
    out.flush()
}

fn main() {
    let boxed: Box<i32> = 22.into();
    let array: Box<[u8; 3]> = [1, 2, 3].into();
    let slice: Box<[i32]> = vec![4, 5].into();
    println!("{} {:?} {:?} {}", boxed, array, slice, Box::from(5u8));

    let text: &str = Default::default();
    let none: &[i32] = Default::default();
    let zeros: [u16; 3] = Default::default();
    let pair: (bool, String) = Default::default();
    let boxed_str: Box<str> = Default::default();
    let boxed_slice: Box<[bool]> = Default::default();
    let marker: std::marker::PhantomData<u8> = Default::default();
    println!("[{}] {:?} {:?} {:?} [{}] {:?} {:?}", text, none, zeros, pair, boxed_str, boxed_slice, marker);

    println!("{}", std::any::type_name::<Vec<Option<&str>>>());
    println!("{} {}", std::any::type_name::<(u8, f64)>(), std::any::type_name_of_val(&[1i64, 2]));

    let mut out = io::stdout();
    writeln!(out, "hello {}", "world").unwrap();
    write!(out, "no newline, ").expect("a write to stdout");
    writeln!(out, "then one").unwrap();
    table(4).unwrap();
    greet(&mut io::stderr()).unwrap();
    println!("end");
}
