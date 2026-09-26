// A tokenizer, walking its source with a `Peekable`: `peek()`, `next()`,
// `next_if` and `next_if_eq`; and iterators that `next()` steps through,
// then hands the rest of (ADR 0071).

use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(u32),
    Ident(String),
    Sym(char),
    Str(String),
}

pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    line: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer {
            chars: src.chars().peekable(),
            line: 1,
        }
    }
    fn number(&mut self, first: char) -> u32 {
        let mut n = first.to_digit(10).unwrap();
        while let Some(&c) = self.chars.peek() {
            match c.to_digit(10) {
                Some(d) => {
                    n = n * 10 + d;
                    self.chars.next();
                }
                None => break,
            }
        }
        n
    }
    fn word(&mut self, first: char) -> String {
        let mut w = String::from(first);
        while let Some(c) = self.chars.next_if(|c| c.is_alphanumeric() || *c == '_') {
            w.push(c);
        }
        w
    }
    pub fn tokens(&mut self) -> Result<Vec<Tok>, String> {
        let mut out = Vec::new();
        while let Some(c) = self.chars.next() {
            match c {
                '\n' => self.line += 1,
                c if c.is_whitespace() => {}
                '0'..='9' => {
                    let n = self.number(c);
                    out.push(Tok::Num(n));
                }
                'a'..='z' | 'A'..='Z' | '_' => {
                    let w = self.word(c);
                    out.push(Tok::Ident(w));
                }
                '"' => {
                    let mut s = String::new();
                    loop {
                        match self.chars.next() {
                            Some('"') => break,
                            Some(ch) => s.push(ch),
                            None => return Err(format!("line {}: unterminated string", self.line)),
                        }
                    }
                    out.push(Tok::Str(s));
                }
                '/' if self.chars.next_if_eq(&'/').is_some() => while self.chars.next_if(|&c| c != '\n').is_some() {},
                c => out.push(Tok::Sym(c)),
            }
        }
        Ok(out)
    }
}

pub fn pairs(v: &[u32]) -> (Vec<(u32, u32)>, Vec<u32>) {
    let mut it = v.iter();
    let mut out = vec![];
    while let (Some(&a), Some(&b)) = (it.next(), it.next()) {
        out.push((a, b));
    }
    let rest: Vec<u32> = it.copied().collect();
    (out, rest)
}

pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn report() -> String {
    let mut out = String::new();
    for src in ["let x_1 = 42 + y;\n// note\nprint(\"hi there\") 7", "a \"open", "  "] {
        out += &format!("{:?}\n", Lexer::new(src).tokens());
    }
    out += &format!("{:?} {:?}\n", pairs(&[1, 2, 3, 4, 5]), pairs(&[]));
    out += &format!("{} {} {:?}\n", capitalize("hello"), capitalize("ßig"), capitalize(""));
    let mut words = "one two three four".split(' ');
    let first = words.next();
    let rest: Vec<&str> = words.collect();
    out += &format!("{first:?} {rest:?}\n");
    out
}
