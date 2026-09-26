// Rows of a CSV parsed into structs, with an error enum that `?` converts
// into, then grouped, sorted and laid out in columns: `map(str::trim)`, a
// `char`'s `to_uppercase()`, `match` on a tuple of ranges, and `{:?}` of a
// parse error (ADR 0070).

use std::collections::BTreeMap;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub team: String,
    pub score: u32,
    pub age: u32,
}

#[derive(Debug)]
pub enum RowError {
    Fields(usize),
    Number(ParseIntError),
    Empty(&'static str),
}

impl From<ParseIntError> for RowError {
    fn from(e: ParseIntError) -> Self {
        RowError::Number(e)
    }
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RowError::Fields(n) => write!(f, "expected 4 fields, got {n}"),
            RowError::Number(e) => write!(f, "bad number: {e}"),
            RowError::Empty(what) => write!(f, "empty {what}"),
        }
    }
}

const HEADERS: [&str; 4] = ["name", "team", "score", "age"];

fn parse_row(line: &str) -> Result<Row, RowError> {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(RowError::Fields(parts.len()));
    }
    if parts[0].is_empty() {
        return Err(RowError::Empty("name"));
    }
    let score = parts[2].parse()?;
    let age: u32 = parts[3].parse()?;
    Ok(Row {
        name: parts[0].to_string(),
        team: parts[1].to_owned(),
        score,
        age,
    })
}

fn capitalize(s: &str) -> String {
    s.chars()
        .take(1)
        .flat_map(|c| c.to_uppercase())
        .chain(s.chars().skip(1))
        .collect()
}

fn grade(score: u32, age: u32) -> &'static str {
    match (score, age) {
        (90.., _) => "A",
        (70..=89, a) if a < 30 => "B+",
        (70..=89, _) => "B",
        _ => "C",
    }
}

pub fn table(input: &str) -> String {
    let mut out = String::new();
    let mut rows = Vec::new();
    for (i, line) in input.lines().enumerate().skip(1) {
        match parse_row(line) {
            Ok(r) => rows.push(r),
            Err(e) => out += &format!("line {}: {e}\n", i + 1),
        }
    }
    rows.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    out += &format!("{:<8}|{:>6}|{:^7}|{}\n", HEADERS[0], HEADERS[2], "grade", HEADERS[1]);
    for r in &rows {
        out += &format!(
            "{:<8}|{:>6}|{:^7}|{}\n",
            capitalize(&r.name),
            r.score,
            grade(r.score, r.age),
            r.team.to_uppercase()
        );
    }
    let mut by_team: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for r in &rows {
        by_team.entry(r.team.as_str()).or_default().push(r);
    }
    for (team, members) in &by_team {
        let total: u32 = members.iter().map(|r| r.score).sum();
        let avg = total as f64 / members.len() as f64;
        let best = members
            .iter()
            .max_by_key(|r| (r.score, std::cmp::Reverse(r.age)))
            .map(|r| r.name.as_str())
            .unwrap_or("-");
        out += &format!("{team:>6}: n={} avg={avg:>6.1} best={best}\n", members.len());
    }
    let ages: Vec<u32> = rows.iter().map(|r| r.age).collect();
    let gaps: Vec<i32> = ages.windows(2).map(|w| w[1] as i32 - w[0] as i32).collect();
    out += &format!("{gaps:?}\n");
    out += &format!(
        "{} {} {:?}\n",
        rows.iter().all(|r| r.score > 50),
        rows.iter().any(|r| r.team == "red"),
        rows.iter().position(|r| r.age > 40)
    );
    out
}

pub fn parse_errors() -> String {
    format!(
        "{:?} {:?} {:?} {:?} {:?} {:?}",
        "".parse::<u8>().unwrap_err(),
        "300".parse::<u8>().unwrap_err(),
        "-40000".parse::<i16>().unwrap_err(),
        "x".parse::<f64>().unwrap_err(),
        "no".parse::<bool>().unwrap_err(),
        "ab".parse::<char>().unwrap_err()
    )
}

pub fn function_values(words: &[&str]) -> (Vec<String>, Vec<bool>, Vec<f64>, Vec<String>) {
    let upper = words.iter().map(|w| w.to_uppercase()).collect();
    let spaces = "a b\tc".chars().map(char::is_whitespace).collect();
    let roots = [4.0, 2.25, 9.0].iter().copied().map(f64::sqrt).collect();
    let owned = words.iter().copied().map(String::from).collect();
    (upper, spaces, roots, owned)
}

pub fn report() -> String {
    let input = "name,team,score,age\nann, red, 91, 34\nbob,blue,78,25\ncy,red,78,45\n,blue,5,5\ndee,blue,x,3\neve,green,64,29\nfay,red\ngus, blue ,88,31\nßen,red,70,20\n";
    format!(
        "{}{}\n{:?}\n",
        table(input),
        parse_errors(),
        function_values(&["x", "é"])
    )
}
