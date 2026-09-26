// A JSON-like value tree: a recursive enum with a `Vec` and a `BTreeMap`,
// `Option` chains through it, a pretty-printer that writes to a `&mut String`,
// and `&mut` to numbers, fields and items handed to functions (ADR 0074).

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(m) => m.get(key),
            _ => None,
        }
    }
    pub fn at(&self, i: usize) -> Option<&Value> {
        if let Value::List(v) = self { v.get(i) } else { None }
    }
    pub fn as_num(&self) -> Option<f64> {
        if let Value::Num(n) = self { Some(*n) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    fn depth(&self) -> usize {
        match self {
            Value::List(v) => 1 + v.iter().map(|x| x.depth()).max().unwrap_or(0),
            Value::Obj(m) => 1 + m.values().map(Value::depth).max().unwrap_or(0),
            _ => 0,
        }
    }
    fn pretty(&self, indent: usize, out: &mut String) {
        let pad = " ".repeat(indent);
        match self {
            Value::List(v) if v.is_empty() => out.push_str("[]"),
            Value::List(v) => {
                out.push_str("[\n");
                for (i, x) in v.iter().enumerate() {
                    out.push_str(&pad);
                    out.push_str("  ");
                    x.pretty(indent + 2, out);
                    if i + 1 < v.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            }
            Value::Obj(m) => {
                out.push('{');
                let mut first = true;
                for (k, x) in m {
                    if !first {
                        out.push_str(", ");
                    }
                    first = false;
                    out.push_str(&format!("{k:?}: "));
                    x.pretty(indent, out);
                }
                out.push('}');
            }
            other => out.push_str(&other.to_string()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::List(v) => {
                write!(f, "[")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Value::Obj(m) => {
                write!(f, "{{")?;
                for (i, (k, x)) in m.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{k:?}:{x}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

fn price(v: &Value) -> Option<f64> {
    let item = v.get("items")?.at(1)?;
    let p = item.get("price")?.as_num()?;
    Some(p * v.get("qty").and_then(Value::as_num).unwrap_or(1.0))
}

pub fn tree() -> String {
    let mut shop = BTreeMap::new();
    shop.insert("name".to_string(), Value::Str("corner \"shop\"".into()));
    shop.insert("open".to_string(), Value::Bool(true));
    shop.insert("qty".to_string(), Value::Num(3.0));
    let mut apple = BTreeMap::new();
    apple.insert("price".to_string(), Value::Num(0.5));
    let mut pear = BTreeMap::new();
    pear.insert("price".to_string(), Value::Num(1.25));
    pear.insert(
        "tags".to_string(),
        Value::List(vec![Value::Str("ripe".into()), Value::Null]),
    );
    shop.insert(
        "items".to_string(),
        Value::List(vec![Value::Obj(apple), Value::Obj(pear), Value::List(vec![])]),
    );
    let v = Value::Obj(shop);
    let mut out = String::new();
    out += &format!("{v}\n");
    let mut p = String::new();
    v.pretty(0, &mut p);
    out += &format!("{p}\n");
    let name = v.get("name").and_then(|n| n.as_str()).map(|s| s.to_uppercase());
    out += &format!(
        "{:?} {:?} {} {}\n",
        price(&v),
        name,
        v.depth(),
        v.get("missing").is_none_or(|x| x.is_null())
    );
    let copy = v.clone();
    out += &format!("{} {}\n", copy == v, Value::Num(1.0) == Value::Num(1.0));
    out
}

// `&mut` to what JS can't change in place: a box, copied back after the call.
fn bump(count: &mut u32, by: u32) -> u32 {
    *count += by;
    *count
}
fn log(out: &mut String, line: &str, count: &mut u32) {
    if !out.is_empty() {
        out.push('|');
    }
    out.push_str(line);
    bump(count, 1);
}
fn settle(slot: &mut Option<u32>) {
    *slot = Some(slot.unwrap_or(0) + 10);
}

pub struct Stats {
    pub hits: u32,
    pub last: Option<u32>,
}

pub fn boxes() -> String {
    let mut n = 0;
    let first = bump(&mut n, 5);
    let total = first + bump(&mut n, 2) + n;
    let mut text = String::new();
    for w in ["a", "b", "c"] {
        log(&mut text, w, &mut n);
    }
    let mut stats = Stats { hits: 1, last: None };
    bump(&mut stats.hits, 4);
    settle(&mut stats.last);
    settle(&mut stats.last);
    let mut counts = vec![1u32, 2, 3];
    bump(&mut counts[1], 40);
    format!("{first} {total} {n} {text} {} {:?} {counts:?}", stats.hits, stats.last)
}

pub fn report() -> String {
    format!("{}{}\n", tree(), boxes())
}
