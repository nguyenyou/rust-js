// A client reading what its server sends, as serde_json reads it (ADR
// 0078): each case prints the value `from_str` gives, or its error, whose
// message and place must be native Rust's to the byte. Structs, enums,
// renames, aliases, defaults, unknown fields, and JSON that's wrong in
// every way serde_json tells apart.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub order_id: u32,
    pub item_name: String,
    #[serde(default)]
    pub qty: u32,
    pub note: Option<String>,
    pub price: f64,
    pub tags: Vec<String>,
    #[serde(rename = "ok", alias = "paid")]
    pub paid: bool,
    #[serde(skip)]
    pub cache: u32,
}

#[derive(Deserialize, Debug)]
pub enum Shape {
    Dot,
    Circle(f64),
    Rect(f64, f64),
    Poly { sides: u32, name: String },
}

#[derive(Deserialize, Debug)]
pub struct Id(pub u32);

#[derive(Deserialize, Debug)]
pub struct Point(pub i32, pub i32);

#[derive(Deserialize, Debug)]
pub struct Unit;

#[derive(Deserialize, Debug)]
#[serde(transparent)]
pub struct Meters {
    pub value: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Strict {
    pub a: u8,
    pub b: Option<i8>,
}

#[derive(Deserialize, Debug)]
pub enum Level {
    #[serde(rename = "lo")]
    Low,
    High,
    #[serde(other)]
    Unknown,
}


fn seven() -> u32 {
    7
}

fn fallback() -> Config {
    Config { name: "fallback".to_string(), level: 3, verbose: true }
}

#[derive(Deserialize, Debug)]
#[serde(default = "fallback")]
pub struct Config {
    pub name: String,
    pub level: u32,
    pub verbose: bool,
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct Loose {
    pub a: u32,
    pub b: String,
    #[serde(default = "seven")]
    pub c: u32,
    #[serde(skip_deserializing)]
    pub d: Vec<u8>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Event {
    #[serde(alias = "begin", alias = "Aa")]
    Started,
    Moved { delta_x: i32, delta_y: i32 },
    #[serde(rename_all = "UPPERCASE")]
    Renamed { old_name: String },
    #[serde(skip_deserializing)]
    Hidden,
}

#[derive(Deserialize, Debug)]
#[serde(expecting = "a point, like [1, 2]")]
pub struct Pt(pub i32, pub i32);

#[derive(Deserialize, Debug)]
pub struct Named {
    #[serde(rename(serialize = "out", deserialize = "in"))]
    pub value: u8,
    #[serde(rename(serialize = "only_out"))]
    pub other: u8,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Nothing {
    #[serde(skip)]
    pub x: u32,
}

#[derive(Deserialize, Debug)]
pub struct Tree {
    pub value: u32,
    pub children: Vec<Tree>,
}

#[derive(Deserialize, Debug)]
pub enum Never {}

#[derive(Deserialize, Debug)]
pub struct Holder {
    pub boxed: Box<u32>,
    pub shared: Box<String>,
    pub queue: VecDeque<i16>,
    pub set: BTreeSet<char>,
    pub by_char: HashMap<char, u8>,
    pub by_int: HashMap<i32, f64>,
    pub nested: Option<Vec<Option<Pt>>>,
}

macro_rules! show {
    ($out:ident, $ty:ty, $($text:expr),*) => {
        $(
            match serde_json::from_str::<$ty>($text) {
                Ok(v) => $out.push_str(&format!("ok {:?}\n", v)),
                Err(e) => $out.push_str(&format!("err {} / {:?}\n", e, e)),
            }
        )*
    };
}

pub fn report() -> String {
    let mut out = String::new();
    show!(out, Order,
        r#"{"orderId":7,"itemName":"pen","qty":3,"note":null,"price":2.5,"tags":["a","b"],"ok":true}"#,
        r#"{"orderId":7,"itemName":"pen","price":2.5,"tags":[],"paid":false,"extra":{"x":[1,2,{"y":null}]}}"#,
        r#"{"orderId":-1,"itemName":"pen"}"#,
        r#"{"orderId":7.5}"#,
        r#"{"orderId":7,"orderId":8}"#,
        r#"{"itemName":"x"}"#,
        r#"[7,"pen",3,null,2.5,[],true]"#,
        r#"[7,"pen"]"#,
        r#"{"orderId":7,"#,
        r#"{"orderId" 7}"#,
        r#"{"orderId":7,}"#,
        r#""x""#,
        "{\n  \"itemName\": \"é\",\n  \"orderId\": \"7\"\n}",
        r#"{"orderId":1e3}"#,
        r#"{"orderId":1,"itemName":"aé😀\n\"q\"","price":1,"tags":[],"ok":true}"#,
        r#"{"itemName":"\x"}"#,
        r#"{"itemName":"\ud800"}"#,
        r#"{"itemName":"\udc00"}"#,
        "{\"itemName\":\"a\u{1}\"}",
        r#"{"orderId":4294967296}"#,
        r#"{"orderId":18446744073709551616}"#,
        r#"{"orderId":null}"#,
        r#"{"orderId":true}"#,
        r#"{"orderId":[1]}"#,
        r#"{"orderId":{}}"#,
        r#"{"orderId":01}"#,
        r#"{"orderId":1.}"#,
        r#"{"orderId":-}"#,
        r#"{"orderId":1e}"#,
        r#"{"orderId":tru}"#,
        r#"{"orderId":7,"tags":[1,]}"#,
        r#"{"orderId":7 "x":1}"#,
        r#"{7:1}"#,
        r#"{"orderId":7,"extra":[1 2]}"#,
        "",
        "   ",
        r#"{"orderId":7,"itemName":"p","price":1,"tags":[],"ok":true} x"#
    );
    show!(out, Shape,
        r#""Dot""#, r#"{"Circle":1.5}"#, r#"{"Rect":[1,2]}"#, r#"{"Poly":{"sides":6,"name":"hex"}}"#,
        r#""Circle""#, r#"{"Nope":1}"#, r#"{"Dot":null}"#, r#"{"Dot":1}"#, r#"{"Circle":1.5,"x":1}"#,
        r#"{}"#, r#"5"#, r#"{"Rect":[1]}"#, r#"{"Poly":[6]}"#, r#"{"Poly":{"sides":6}}"#, r#""Poly""#, r#""Rect""#
    );
    show!(out, Id, "5", r#""5""#, "5 x", "[5]");
    show!(out, Point, "[1,-2]", "[1]", "[1,2,3]", r#"{"a":1}"#, "[1,2,]", "[-2147483649,0]");
    show!(out, Unit, "null", "1");
    show!(out, Meters, "3.5", "3", "-0");
    show!(out, Strict, r#"{"a":1,"c":2}"#, r#"{"a":300}"#, r#"{"a":1,"b":-129}"#, r#"{"a":1,"b":-128}"#);
    show!(out, Level, r#""lo""#, r#""High""#, r#""zzz""#, r#""Low""#, r#"{"lo":null}"#);
    show!(out, BTreeMap<String, u32>, r#"{"b":1,"a":2,"b":3}"#);
    show!(out, BTreeMap<u32, bool>, r#"{"10":true,"2":false}"#, r#"{"x":true}"#, r#"{"1 ":true}"#, r#"{"-1":true}"#);
    show!(out, BTreeMap<bool, u8>, r#"{"true":1,"false":2}"#, r#"{"yes":1}"#);
    show!(out, Vec<Option<f64>>, "[1,null,2.5e-3,-0]", "[1,null", "[,1]");
    show!(out, (u8, String, bool), r#"[255,"x",true]"#, r#"[256,"x",true]"#, r#"[1]"#);
    show!(out, [u8; 3], "[1,2,3]", "[1,2]");
    show!(out, f64, "1e400", "123456789012345678901234567890", "0.1", "-0", "1.7976931348623157e308",
        "4.9e-324", "2.2250738585072014e-308", "0.30000000000000004", "1.5E+3", "-1e-400", "1e-400",
        "18446744073709551615", "18446744073709551616", "-9223372036854775808", "-9223372036854775809",
        "123.456e-2", "1234567890123456789012.5", "0.000001", "9007199254740993", "1e2147483648", "0e99999999999");
    show!(out, char, r#""é""#, r#""ab""#, r#""""#);
    show!(out, i8, "-128", "-129", "128", "1.0");
    show!(out, String, "5", "true", "null", "[]", "{}", r#""\/\b\f\r\t""#);
    show!(out, bool, "true", "false", "tru", "nul");
    show!(out, Config, "{}", r#"{"name":"x"}"#, r#"{"level":1,"verbose":false,"name":"y"}"#, "[]", r#"["z"]"#);
    show!(out, Loose, "{}", r#"{"c":1,"d":[1]}"#, "[5]", r#"[5,"b",9]"#, r#"[5,"b",9,1]"#);
    show!(out, Event, r#""started""#, r#""begin""#, r#""Aa""#, r#""hidden""#, r#""Hidden""#, r#""nope""#,
        r#"{"moved":{"deltaX":1,"deltaY":-1}}"#, r#"{"moved":{"deltaX":1,"delta_y":-1}}"#,
        r#"{"renamed":{"OLD_NAME":"a"}}"#, r#"{"renamed":{"oldName":"a"}}"#, r#"{"moved":[1,2]}"#, r#"{"moved":[1]}"#);
    show!(out, Pt, "[1,2]", "[1]", "{}", "3");
    show!(out, Named, r#"{"in":1,"other":2}"#, r#"{"out":1,"other":2}"#, r#"{"in":1,"only_out":2}"#);
    show!(out, Nothing, "{}", r#"{"x":1}"#);
    show!(out, Never, r#""a""#, "{}");
    show!(out, Tree, r#"{"value":1,"children":[{"value":2,"children":[]}]}"#);
    let mut deep = String::new();
    for _ in 0..130 {
        deep.push_str(r#"{"value":0,"children":["#);
    }
    show!(out, Tree, &deep);
    let mut ignored = String::from(r#"{"value":0,"children":[],"skip":"#);
    for _ in 0..200 {
        ignored.push('[');
    }
    for _ in 0..200 {
        ignored.push(']');
    }
    ignored.push('}');
    show!(out, Tree, &ignored);
    show!(out, Holder,
        r#"{"boxed":5,"shared":"s","queue":[-1,2],"set":["b","a","b"],"by_char":{"x":1},"by_int":{"-3":1.5},"nested":[null,[1,2]]}"#,
        r#"{"boxed":5,"shared":"s","queue":[],"set":[],"by_char":{"xy":1},"by_int":{},"nested":null}"#,
        r#"{"boxed":5,"shared":"s","queue":[],"set":[],"by_char":{},"by_int":{"1.5":1},"nested":null}"#,
        r#"{"boxed":5,"shared":"s","queue":[],"set":[],"by_char":{},"by_int":{},"nested":[[1]]}"#
    );
    let text = String::from("[1, 2]");
    show!(out, Pt, &text);
    show!(out, Vec<u32>, "[1,\n2,\n  x]", "[\r\n1\t,2  ]", "\n\n  [1,2", "[1]\n\n  ,");
    out
}
