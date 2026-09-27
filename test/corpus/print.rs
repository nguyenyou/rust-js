// What `println!` and its kin write, to stdout and stderr, lines and parts
// of lines (ADR 0087).
fn main() {
    let n = 7;
    println!("hello");
    println!("n = {n}, twice = {}", n * 2);
    println!();
    print!("a");
    print!("b {}", n);
    println!(" c");
    print!("ends here\n");
    eprintln!("warn {:?}", vec![1, 2]);
    eprint!("partial ");
    eprintln!("done");
    println!("100% {{literal}} %s %d");
    let name = "x";
    println!("{name:>5}|{:<4}|{:^7}|", 12, "mid");
    println!("tab\tquote\" backslash\\ unicode é 🦀");
}
