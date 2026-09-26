// A calculator: a tokenizer, the shunting-yard algorithm and a stack
// machine, with word counts and a Caesar cipher. What a program that reads
// text does: `?` from a `&str` error to a `String` one, `+=` on a
// `String`, `split` by a closure, and `for (i, c)` over `enumerate()`.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token::Num(text.parse().map_err(|_| format!("bad number {text}"))?));
            continue;
        }
        match c {
            '+' | '-' | '*' | '/' => out.push(Token::Op(c)),
            '(' => out.push(Token::LParen),
            ')' => out.push(Token::RParen),
            _ => return Err(format!("unexpected {c:?} at {i}")),
        }
        i += 1;
    }
    Ok(out)
}

fn prec(op: char) -> u8 {
    if op == '+' || op == '-' { 1 } else { 2 }
}

pub fn to_rpn(tokens: &[Token]) -> Vec<Token> {
    let mut out = vec![];
    let mut stack: Vec<Token> = vec![];
    for t in tokens {
        match t {
            Token::Num(_) => out.push(t.clone()),
            Token::Op(o) => {
                while let Some(Token::Op(top)) = stack.last() {
                    if prec(*top) >= prec(*o) {
                        out.push(stack.pop().unwrap());
                    } else {
                        break;
                    }
                }
                stack.push(t.clone());
            }
            Token::LParen => stack.push(Token::LParen),
            Token::RParen => {
                while let Some(top) = stack.pop() {
                    if top == Token::LParen {
                        break;
                    }
                    out.push(top);
                }
            }
        }
    }
    while let Some(t) = stack.pop() {
        out.push(t);
    }
    out
}

pub fn eval(src: &str) -> Result<f64, String> {
    let rpn = to_rpn(&tokenize(src)?);
    let mut st: Vec<f64> = Vec::new();
    for t in &rpn {
        match *t {
            Token::Num(n) => st.push(n),
            Token::Op(o) => {
                let b = st.pop().ok_or("underflow")?;
                let a = st.pop().ok_or("underflow")?;
                st.push(match o {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    _ => unreachable!(),
                });
            }
            _ => return Err("paren".into()),
        }
    }
    st.pop().ok_or_else(|| "empty".to_string())
}

pub fn word_freq(text: &str) -> Vec<(String, usize)> {
    let mut m: HashMap<String, usize> = HashMap::new();
    for w in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        *m.entry(w.to_lowercase()).or_insert(0) += 1;
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

pub fn caesar(s: &str, k: u8) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                (((c as u8 - b'a' + k) % 26) + b'a') as char
            } else if c.is_ascii_uppercase() {
                (((c as u8 - b'A' + k) % 26) + b'A') as char
            } else {
                c
            }
        })
        .collect()
}

pub fn first_dup(s: &str) -> Option<(usize, char)> {
    let mut seen = HashSet::new();
    for (i, c) in s.chars().enumerate() {
        if !seen.insert(c) {
            return Some((i, c));
        }
    }
    None
}

pub fn report() -> String {
    let mut out = String::new();
    for e in [
        "1 + 2 * 3",
        "(1 + 2) * 3",
        "10 / 4 - 1",
        "2 * (3 + 4) * 5",
        "1 +",
        "3 $ 4",
    ] {
        out += &format!("{e} = {:?}\n", eval(e));
    }
    out += &format!("{:?}\n", word_freq("The cat and the hat. THE end, and fin"));
    out += &format!("{} {}\n", caesar("Hello, World!", 3), caesar(&caesar("abcxyz", 13), 13));
    out += &format!("{:?} {:?}\n", first_dup("abcdbe"), first_dup("xyz"));
    let s = "hello world";
    out += &format!(
        "{:?} {:?} {} {}\n",
        s.split('o').count(),
        s.split(|c: char| c == 'l').collect::<Vec<_>>(),
        s.chars().rev().collect::<String>(),
        s.chars().filter(|c| "aeiou".contains(*c)).count()
    );
    out
}
