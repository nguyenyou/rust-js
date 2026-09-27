// An API's shared types, read and written as serde does (ADRs 0077 to
// 0082): generic responses, `Result`s, types that convert to and from
// what's on the wire, `#[serde(from, try_from, into)]`, flattened fields,
// and generic functions that encode and decode any of them.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Debug;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_item: Option<T>,
    pub total: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Pair<A, B> {
    pub left: A,
    pub right: B,
    pub by_name: BTreeMap<String, B>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Reply<T> {
    Ok(T),
    Err { message: String },
    Empty,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Status<T> {
    Done { data: T },
    Failed { reason: String },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "t", content = "c")]
pub enum Tagged<T> {
    One(T),
    Many(Vec<T>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Wrap<T>(pub T);

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub name: String,
    pub age: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Outcome {
    pub result: Result<u32, String>,
    pub all: Vec<Result<bool, u8>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(from = "RawTemp", into = "RawTemp")]
pub struct Temp {
    pub celsius: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RawTemp {
    pub c: f64,
}

impl From<RawTemp> for Temp {
    fn from(raw: RawTemp) -> Self {
        Temp { celsius: raw.c }
    }
}

impl From<Temp> for RawTemp {
    fn from(temp: Temp) -> Self {
        RawTemp { c: temp.celsius }
    }
}

#[derive(Deserialize, Debug)]
#[serde(try_from = "String")]
pub struct Email(pub String);

impl TryFrom<String> for Email {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        if s.contains('@') { Ok(Email(s)) } else { Err(format!("`{s}` is not an email")) }
    }
}

pub struct OddError(u32);

impl fmt::Display for OddError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} is odd", self.0)
    }
}

#[derive(Deserialize, Debug)]
#[serde(try_from = "u32")]
pub struct Even(pub u32);

impl TryFrom<u32> for Even {
    type Error = OddError;
    fn try_from(n: u32) -> Result<Self, OddError> {
        if n % 2 == 0 { Ok(Even(n)) } else { Err(OddError(n)) }
    }
}

#[derive(Deserialize, Debug)]
pub struct Contact {
    pub email: Email,
    pub temp: Temp,
    pub even: Even,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response<T> {
    pub data: T,
    pub ok: bool,
}

pub fn encode<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

pub fn roundtrip<T: Serialize + DeserializeOwned + Debug>(value: &T) -> String {
    let text = encode(value);
    let back: T = decode(&text).unwrap();
    format!("{text} {back:?}")
}

pub fn decode_all<T>(texts: &[&str]) -> Vec<Result<T, String>>
where
    T: DeserializeOwned,
{
    texts.iter().map(|t| decode(t)).collect()
}

pub fn borrowed<'de, T: Deserialize<'de>>(text: &'de str) -> Option<T> {
    serde_json::from_str(text).ok()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meta {
    pub page: u32,
    pub total: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Listing {
    pub name: String,
    #[serde(flatten)]
    pub meta: Meta,
    #[serde(flatten)]
    pub extra: BTreeMap<String, u32>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct StrictListing {
    pub name: String,
    #[serde(flatten)]
    pub meta: Meta,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WithOption {
    pub id: u8,
    #[serde(flatten)]
    pub meta: Option<Meta>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Kind {
    Book { pages: u32 },
    Film(u32),
    Pair(u8, u8),
    Other,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Item {
    pub id: u8,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Shape {
    Circle { r: f64 },
    Square { side: f64 },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Drawing {
    pub name: String,
    #[serde(flatten)]
    pub shape: Shape,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Nested {
    #[serde(flatten)]
    pub listing: Listing,
    pub flag: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Bad {
    pub id: u8,
    #[serde(flatten)]
    pub n: u32,
}

#[derive(Serialize, Debug)]
pub struct BadSeq {
    #[serde(flatten)]
    pub items: Vec<u8>,
}

macro_rules! flat_round {
    ($out:ident, $ty:ty, $($text:expr),*) => {
        $(
            match serde_json::from_str::<$ty>($text) {
                Ok(v) => $out.push_str(&format!("ok {:?} -> {}\n", v, match serde_json::to_string(&v) { Ok(s) => s, Err(e) => format!("err {}", e) })),
                Err(e) => $out.push_str(&format!("err {}\n", e)),
            }
        )*
    };
}

macro_rules! round {
    ($out:ident, $ty:ty, $($text:expr),*) => {
        $(
            match serde_json::from_str::<$ty>($text) {
                Ok(v) => $out.push_str(&format!("ok {:?} -> {}\n", v, serde_json::to_string(&v).unwrap())),
                Err(e) => $out.push_str(&format!("err {}\n", e)),
            }
        )*
    };
}

pub fn report() -> String {
    let mut out = String::new();
    round!(out, Page<u32>, r#"{"items":[1,2],"nextItem":3,"total":2}"#, r#"{"items":[],"nextItem":null,"total":0}"#,
        r#"{"items":["x"],"total":1}"#);
    round!(out, Page<User>, r#"{"items":[{"name":"a","age":3}],"nextItem":null,"total":1}"#,
        r#"{"items":[{"name":"a"}],"total":1}"#);
    round!(out, Page<Page<u8>>, r#"{"items":[{"items":[1],"total":1}],"nextItem":{"items":[],"total":0},"total":1}"#);
    round!(out, Page<Option<u8>>, r#"{"items":[null,1],"nextItem":null,"total":2}"#);
    round!(out, Pair<String, f64>, r#"{"left":"l","right":1.5,"by_name":{"a":2}}"#, r#"{"left":1,"right":1.5,"by_name":{}}"#);
    round!(out, Reply<User>, r#"{"Ok":{"name":"b","age":4}}"#, r#"{"Err":{"message":"no"}}"#, r#""Empty""#, r#"{"Ok":5}"#);
    round!(out, Status<Vec<u8>>, r#"{"status":"done","data":[1,2]}"#, r#"{"status":"failed","reason":"x"}"#, r#"{"data":[1],"status":"done"}"#);
    round!(out, Status<User>, r#"{"status":"done","data":{"name":"c","age":5}}"#);
    round!(out, Tagged<bool>, r#"{"t":"One","c":true}"#, r#"{"c":[true,false],"t":"Many"}"#);
    round!(out, Either<u8, String>, "5", r#""s""#, "true");
    round!(out, Wrap<Pair<u8, u8>>, r#"{"left":1,"right":2,"by_name":{"k":3}}"#);
    let page = Page { items: vec![User { name: "z".into(), age: 9 }], next_item: None, total: 1 };
    out.push_str(&serde_json::to_string_pretty(&page).unwrap());
    out.push('\n');
    for text in [
        r#"{"result":{"Ok":5},"all":[{"Ok":true},{"Err":3}]}"#,
        r#"{"result":{"Err":"bad"},"all":[]}"#,
        r#"{"result":"Ok","all":[]}"#,
        r#"{"result":{"Maybe":1},"all":[]}"#,
        r#"{"result":{"Ok":"x"},"all":[]}"#,
    ] {
        match serde_json::from_str::<Outcome>(text) {
            Ok(v) => out.push_str(&format!("ok {:?} -> {}\n", v, serde_json::to_string(&v).unwrap())),
            Err(e) => out.push_str(&format!("err {}\n", e)),
        }
    }
    for text in [
        r#"{"email":"a@b","temp":{"c":21.5},"even":4}"#,
        r#"{"email":"ab","temp":{"c":21.5},"even":4}"#,
        r#"{"email":"a@b","temp":{"c":21.5},"even":3}"#,
        r#"{"email":"a@b","temp":{"celsius":21.5},"even":4}"#,
        r#"{"email":5,"temp":{"c":1},"even":4}"#,
    ] {
        match serde_json::from_str::<Contact>(text) {
            Ok(v) => out.push_str(&format!("ok {:?}\n", v)),
            Err(e) => out.push_str(&format!("err {}\n", e)),
        }
    }
    for text in [r#""x@y""#, r#""xy""#] {
        match serde_json::from_str::<Email>(text) {
            Ok(v) => out.push_str(&format!("ok {:?}\n", v)),
            Err(e) => out.push_str(&format!("err {}\n", e)),
        }
    }
    let temp = Temp { celsius: -3.0 };
    out.push_str(&serde_json::to_string(&temp).unwrap());
    out.push('\n');
    out.push_str(&serde_json::to_string(&vec![temp.clone(), temp]).unwrap());
    out.push('\n');
    out.push_str(&roundtrip(&User { name: "a".into(), age: 1 }));
    out.push('\n');
    out.push_str(&roundtrip(&vec![1u32, 2]));
    out.push('\n');
    out.push_str(&roundtrip(&Response { data: User { name: "b".into(), age: 2 }, ok: true }));
    out.push('\n');
    let users: Vec<Result<User, String>> = decode_all(&[r#"{"name":"c","age":3}"#, r#"{"name":"d"}"#]);
    out.push_str(&format!("{users:?}\n"));
    let reply: Result<Response<Vec<u8>>, String> = decode(r#"{"data":[1,2],"ok":false}"#);
    out.push_str(&format!("{reply:?}\n"));
    let n: Option<u8> = borrowed("7");
    let m: Option<u8> = borrowed("x");
    out.push_str(&format!("{n:?} {m:?}\n"));
    out.push_str(&encode(&Response { data: (), ok: true }));
    out.push('\n');
    flat_round!(out, Listing, r#"{"name":"a","page":1,"total":9,"x":5}"#, r#"{"page":1,"name":"a","total":9}"#,
        r#"{"name":"a","page":1}"#, r#"{"name":"a","page":1,"total":2,"x":"y"}"#, r#"["a"]"#,
        r#"{"name":"a","page":"1","total":2}"#);
    flat_round!(out, StrictListing, r#"{"name":"a","page":1,"total":2}"#, r#"{"name":"a","page":1,"total":2,"z":0}"#);
    flat_round!(out, WithOption, r#"{"id":1,"page":1,"total":2}"#, r#"{"id":1,"page":1}"#, r#"{"id":1}"#);
    flat_round!(out, Item, r#"{"id":1,"Book":{"pages":3}}"#, r#"{"Film":7,"id":2}"#, r#"{"id":3,"Other":null}"#,
        r#"{"id":4,"Pair":[1,2]}"#, r#"{"id":5}"#, r#"{"id":6,"Nope":1}"#);
    flat_round!(out, Drawing, r#"{"name":"d","type":"Circle","r":1.5}"#, r#"{"type":"Square","side":2,"name":"e"}"#,
        r#"{"name":"f","type":"Hex"}"#, r#"{"name":"g"}"#);
    flat_round!(out, Nested, r#"{"name":"n","page":1,"total":2,"flag":true,"q":3}"#);
    flat_round!(out, Bad, r#"{"id":1,"n":2}"#);
    out.push_str(&match serde_json::to_string(&Bad { id: 1, n: 2 }) { Ok(s) => s, Err(e) => e.to_string() });
    out.push('\n');
    out.push_str(&match serde_json::to_string(&BadSeq { items: vec![1] }) { Ok(s) => s, Err(e) => e.to_string() });
    out.push('\n');
    let listing = Listing { name: "p".into(), meta: Meta { page: 2, total: 3 }, extra: BTreeMap::new() };
    out.push_str(&serde_json::to_string_pretty(&Nested { listing, flag: false }).unwrap());
    out.push('\n');
    out
}
