// Strings (ADR 0034): a `String` or `&str` is a JS string (ADR 0023), and a
// `char` is a string of one character. Their methods are JS's.

/// `format!`, as the pieces and values joined.
pub fn labeled(name: &str, n: u32) -> String {
    format!("{name}: {} item{}", n, if n == 1 { "" } else { "s" })
}

pub fn tests(s: &str) -> (bool, bool, bool, bool) {
    (s.starts_with("ab"), s.ends_with('c'), s.contains("b/"), s.contains('/'))
}

pub fn cases(s: &str) -> (String, String) {
    (s.to_uppercase(), s.to_lowercase())
}

pub fn trimmed(s: &str) -> (String, String) {
    (s.trim_start().to_string(), s.trim_end().to_string())
}

pub fn replaced(s: &str) -> String {
    s.replace("/", " / ").replace('a', "A")
}

/// `strip_prefix` and `strip_suffix` are options.
pub fn module_name(path: &str) -> String {
    let file = match path.strip_prefix("src/") {
        Some(rest) => rest,
        None => path,
    };
    file.strip_suffix(".rs").unwrap_or(file).to_string()
}

/// `split`: looped over, collected, or its last piece.
pub fn parts(path: &str) -> (Vec<String>, u32, String) {
    let mut kept = Vec::new();
    let mut empty = 0;
    for part in path.split('/') {
        if part.is_empty() {
            empty += 1;
        } else {
            kept.push(part.to_string());
        }
    }
    let last = path.split("/").last().unwrap_or("").to_string();
    (kept, empty, last)
}

pub fn rejoined(path: &str) -> String {
    let pieces: Vec<&str> = path.split('/').collect();
    pieces.join(" > ")
}

/// Building a string up: `push_str` and `push` on a variable.
pub fn built(n: u32) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&i.to_string());
        s.push(',');
    }
    s.push('!');
    s
}

/// `char`s: literals, `==`, and `to_string`.
pub fn separator(windows: bool) -> String {
    let c = if windows { '\\' } else { '/' };
    let same = c == '/';
    c.to_string() + if same { " (unix)" } else { " (windows)" }
}

/// `split_once` and `rsplit_once`: an option of the two sides.
pub fn folder_and_file(path: &str) -> (String, String) {
    let first = match path.split_once('/') {
        Some((top, _)) => top.to_string(),
        None => String::new(),
    };
    let file = match path.rsplit_once("/") {
        Some((_, name)) => name,
        None => path,
    };
    (first, file.to_string())
}

pub fn repeated(s: &str, n: u32) -> String {
    s.repeat(n as usize)
}

/// `match` on string literals, with `|`.
pub fn kind(s: &str) -> u32 {
    match s {
        "" => 0,
        "abc" | "stats.rs" => 1,
        "äbc/Ö" => 2,
        "ab/c" => 3,
        _ => 4,
    }
}

/// String literals inside other patterns: an option, a tuple.
pub fn tagged(s: &str) -> (u32, bool) {
    let top = match s.split_once('/') {
        Some((top, _)) => Some(top),
        None => None,
    };
    let n = match (top, s.ends_with('/')) {
        (Some("ab"), _) => 1,
        (Some(""), true) => 2,
        (None, _) => 3,
        _ => 4,
    };
    (n, matches!(top, Some("a" | "src")))
}
