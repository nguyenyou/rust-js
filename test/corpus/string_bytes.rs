// A string's length, slices and offsets count its UTF-8 bytes, as Rust's do.
fn first_word(s: &str) -> &str {
    match s.find(' ') {
        Some(at) => &s[..at],
        None => s,
    }
}

fn main() {
    for w in ["hello", "héllo", "日本語", "🦀 crab", ""] {
        println!("{} {} {}", w, w.len(), w.is_empty());
    }
    let s = String::from("héllo wörld");
    println!("{}", s.len());
    println!("[{}] [{}] [{}] [{}] [{}]", &s[0..1], &s[1..3], &s[7..], &s[..6], &s[..]);
    println!("{}", first_word(&s));
    println!("{:?} {:?} {:?}", s.find('ö'), s.find("wör"), s.find('z'));
    println!("{:?} {:?}", s.rfind('l'), "a🦀b🦀".rfind('🦀'));
    println!("{:?} {:?}", "abc".find(""), "abc".rfind(""));
    for (i, c) in "a日🦀b".char_indices() {
        println!("{} {}", i, c);
    }
    let crab = "🦀🦀";
    println!("{} {} {}", &crab[4..], &crab[..=3], &crab[8..]);
    let mut text = String::new();
    while text.len() < 9 {
        text.push('é');
    }
    println!("{} {}", text, text.len());
    let range = 3..5;
    println!("{}", &s[range]);
    if let Some(at) = s.find('w') {
        let (left, right) = (&s[..at], &s[at..]);
        println!("{}|{} {}", left, right, left.len() + right.len());
    }
}
