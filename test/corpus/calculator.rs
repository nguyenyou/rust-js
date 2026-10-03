// Ordinary Rust, end to end: an interpreter of arithmetic with variables,
// as a person would write one. A tokenizer over a `Peekable<Chars>`, a
// recursive-descent parser into an enum with boxes, errors as an enum with
// `Display`, and an environment in a `HashMap`.
use std::collections::HashMap;
use std::fmt;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
    Assign,
}

#[derive(Debug)]
enum Expr {
    Num(f64),
    Var(String),
    Neg(Box<Expr>),
    Bin(Box<Expr>, char, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, PartialEq)]
enum CalcError {
    Unexpected(char),
    UnexpectedEnd,
    Expected(String),
    Unknown(String),
    DivideByZero,
    Arity { name: String, wanted: usize, got: usize },
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CalcError::Unexpected(c) => write!(f, "unexpected '{c}'"),
            CalcError::UnexpectedEnd => write!(f, "unexpected end of input"),
            CalcError::Expected(what) => write!(f, "expected {what}"),
            CalcError::Unknown(name) => write!(f, "unknown name `{name}`"),
            CalcError::DivideByZero => write!(f, "division by zero"),
            CalcError::Arity { name, wanted, got } => {
                write!(f, "{name} takes {wanted} argument{}, got {got}", if *wanted == 1 { "" } else { "s" })
            }
        }
    }
}

fn number(chars: &mut Peekable<Chars>) -> f64 {
    let mut text = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            text.push(c);
            chars.next();
        } else {
            break;
        }
    }
    text.parse().unwrap_or(0.0)
}

fn tokenize(source: &str) -> Result<Vec<Token>, CalcError> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' => {
                chars.next();
            }
            '0'..='9' | '.' => tokens.push(Token::Num(number(&mut chars))),
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut name = String::new();
                while let Some(c) = chars.next_if(|c| c.is_alphanumeric() || *c == '_') {
                    name.push(c);
                }
                tokens.push(Token::Ident(name));
            }
            '+' | '-' | '*' | '/' | '^' => {
                tokens.push(Token::Op(c));
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            ',' => {
                tokens.push(Token::Comma);
                chars.next();
            }
            '=' => {
                tokens.push(Token::Assign);
                chars.next();
            }
            other => return Err(CalcError::Unexpected(other)),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn expect(&mut self, token: Token, what: &str) -> Result<(), CalcError> {
        match self.next() {
            Some(t) if t == token => Ok(()),
            _ => Err(CalcError::Expected(what.to_string())),
        }
    }

    // expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Result<Expr, CalcError> {
        let mut left = self.term()?;
        while let Some(Token::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.next();
            let right = self.term()?;
            left = Expr::Bin(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    // term := power (('*' | '/') power)*
    fn term(&mut self) -> Result<Expr, CalcError> {
        let mut left = self.power()?;
        while let Some(Token::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.next();
            let right = self.power()?;
            left = Expr::Bin(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    // power := unary ('^' power)?
    fn power(&mut self) -> Result<Expr, CalcError> {
        let base = self.unary()?;
        if let Some(Token::Op('^')) = self.peek() {
            self.next();
            let exponent = self.power()?;
            return Ok(Expr::Bin(Box::new(base), '^', Box::new(exponent)));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Some(Token::Op('-')) => Ok(Expr::Neg(Box::new(self.unary()?))),
            Some(Token::Num(n)) => Ok(Expr::Num(n)),
            Some(Token::Ident(name)) => {
                if let Some(Token::LParen) = self.peek() {
                    self.next();
                    let mut args = Vec::new();
                    if self.peek() != Some(&Token::RParen) {
                        args.push(self.expr()?);
                        while let Some(Token::Comma) = self.peek() {
                            self.next();
                            args.push(self.expr()?);
                        }
                    }
                    self.expect(Token::RParen, "')'")?;
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Some(Token::LParen) => {
                let inner = self.expr()?;
                self.expect(Token::RParen, "')'")?;
                Ok(inner)
            }
            Some(other) => Err(CalcError::Expected(format!("a value, not {other:?}"))),
            None => Err(CalcError::UnexpectedEnd),
        }
    }
}

struct Calculator {
    vars: HashMap<String, f64>,
    functions: HashMap<&'static str, (usize, fn(&[f64]) -> f64)>,
}

impl Calculator {
    fn new() -> Self {
        let mut functions: HashMap<&'static str, (usize, fn(&[f64]) -> f64)> = HashMap::new();
        functions.insert("max", (2, |a| a[0].max(a[1])));
        functions.insert("min", (2, |a| a[0].min(a[1])));
        functions.insert("sqrt", (1, |a| a[0].sqrt()));
        functions.insert("abs", (1, |a| a[0].abs()));
        let mut vars = HashMap::new();
        vars.insert("pi".to_string(), std::f64::consts::PI);
        Calculator { vars, functions }
    }

    fn eval(&self, expr: &Expr) -> Result<f64, CalcError> {
        Ok(match expr {
            Expr::Num(n) => *n,
            Expr::Var(name) => *self.vars.get(name).ok_or_else(|| CalcError::Unknown(name.clone()))?,
            Expr::Neg(inner) => -self.eval(inner)?,
            Expr::Bin(left, op, right) => {
                let (a, b) = (self.eval(left)?, self.eval(right)?);
                match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' if b == 0.0 => return Err(CalcError::DivideByZero),
                    '/' => a / b,
                    _ => a.powf(b),
                }
            }
            Expr::Call(name, args) => {
                let &(wanted, f) = self.functions.get(name.as_str()).ok_or_else(|| CalcError::Unknown(name.clone()))?;
                if args.len() != wanted {
                    return Err(CalcError::Arity { name: name.clone(), wanted, got: args.len() });
                }
                let values = args.iter().map(|a| self.eval(a)).collect::<Result<Vec<_>, _>>()?;
                f(&values)
            }
        })
    }

    fn run(&mut self, line: &str) -> Result<Option<f64>, CalcError> {
        let tokens = tokenize(line)?;
        // `name = expr` assigns.
        if let [Token::Ident(name), Token::Assign, ..] = tokens.as_slice() {
            let name = name.clone();
            let mut parser = Parser { tokens: tokens[2..].to_vec(), at: 0 };
            let value = self.eval(&parser.expr()?)?;
            self.vars.insert(name, value);
            return Ok(None);
        }
        let mut parser = Parser { tokens, at: 0 };
        let expr = parser.expr()?;
        if let Some(extra) = parser.peek() {
            return Err(CalcError::Expected(format!("the end, not {extra:?}")));
        }
        self.eval(&expr).map(Some)
    }
}

fn main() {
    let mut calc = Calculator::new();
    let lines = [
        "1 + 2 * 3",
        "(1 + 2) * 3",
        "2 ^ 3 ^ 2",
        "-4 + 10 / 4",
        "x = 7",
        "y = x * 2 - 1",
        "max(x, y) + min(1, sqrt(16))",
        "abs(-2.5) * pi",
        "10 / (x - 7)",
        "foo + 1",
        "max(1)",
        "3 $ 4",
        "(1 + 2",
        "1 2",
    ];
    for line in lines {
        match calc.run(line) {
            Ok(Some(value)) => println!("{line} => {value}"),
            Ok(None) => println!("{line}"),
            Err(e) => println!("{line} !! {e}"),
        }
    }
    let mut names: Vec<_> = calc.vars.keys().cloned().collect();
    names.sort();
    println!("{}", names.join(", "));
}
