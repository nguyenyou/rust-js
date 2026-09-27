// JSON whose shape isn't known ahead of time: serde_json's `Value`, `Map`
// and `Number` (ADR 0083). Read and written, matched on, indexed, compared,
// built with `json!`, and converted to and from typed values with
// `to_value` and `from_value`, each as native Rust does it.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Number, Value};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct Record {
    pub name: String,
    pub data: Value,
    pub meta: Map<String, Value>,
}

pub fn describe(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("bool {b}"),
        Value::Number(n) => format!("number {n}"),
        Value::String(s) => format!("string {s}"),
        Value::Array(items) => format!("array of {}", items.len()),
        Value::Object(map) => format!("object with {}", map.len()),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub name: String,
    pub age: u8,
    pub tags: Vec<String>,
    #[serde(default)]
    pub extra: Option<Value>,
    pub initial: char,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Shape {
    Dot,
    Circle(f64),
    Rect(f64, f64),
    Poly { sides: u32 },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Tagged {
    Moved { dx: i32 },
    Named(User),
}

macro_rules! from {
    ($out:ident, $ty:ty, $($v:expr),*) => {
        $(
            match serde_json::from_value::<$ty>($v) {
                Ok(x) => $out.push_str(&format!("ok {:?}\n", x)),
                Err(e) => $out.push_str(&format!("err {}\n", e)),
            }
        )*
    };
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "kind")]
pub enum Message {
    Data { payload: Value },
    Empty,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Open {
    pub id: u32,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

pub fn report() -> String {
    let mut out = String::new();
    let texts = [
        r#"{"a":1,"b":[true,null,"x",1.5,-2],"c":{"d":1e3}}"#,
        "1", "1.0", "-0", "18446744073709551615", "\"s\\n\"", "[]", "{}", "[1,", "null", r#"{"z":1,"a":2}"#,
    ];
    for t in texts {
        match serde_json::from_str::<Value>(t) {
            Ok(v) => out.push_str(&format!("{v} | {v:?} | {}\n", describe(&v))),
            Err(e) => out.push_str(&format!("err {e}\n")),
        }
    }
    let v: Value = serde_json::from_str(r#"{"name":"n","list":[1,2,3],"nested":{"k":"v"},"flag":true,"f":2.5}"#).unwrap();
    out.push_str(&format!("{} {} {} {}\n", v["name"], v["list"][1], v["nested"]["k"], v["missing"]));
    out.push_str(&format!("{:?} {:?} {:?} {:?}\n", v.get("name"), v.get("nope"), v["list"].get(5), v["list"].get(0)));
    out.push_str(&format!(
        "{:?} {:?} {:?} {:?}\n",
        v["name"].as_str(),
        v["flag"].as_bool(),
        v["f"].as_f64(),
        v["list"].as_array().map(|a| a.len())
    ));
    out.push_str(&format!(
        "{} {} {} {} {} {}\n",
        v["name"] == "n",
        v["list"][0] == 1,
        v["f"] == 2.5,
        v["flag"] == true,
        v["list"][0] == 1.0,
        v["f"] == 2
    ));
    out.push_str(&format!("{} {} {}\n", v.is_object(), v["list"].is_array(), v["missing"].is_null()));
    let j = json!({"x": 1, "y": [1, "two", null, 3.5], "z": {"w": false}, "s": "str"});
    out.push_str(&format!("{j}\n{j:?}\n"));
    let n = 5u32;
    let name = "abc";
    let k = json!({"n": n, "name": name, "list": vec![1, 2], "opt": Option::<u8>::None});
    out.push_str(&format!("{k}\n"));
    let e = Record { name: "e".into(), data: json!([1, {"a": null}]), meta: Map::new() };
    let text = serde_json::to_string(&e).unwrap();
    out.push_str(&format!("{text}\n"));
    let back: Record = serde_json::from_str(&text).unwrap();
    out.push_str(&format!("{back:?}\n"));
    let mut m = Map::new();
    m.insert("b".to_string(), Value::from(2));
    m.insert("a".to_string(), Value::from("x"));
    m.insert("c".to_string(), Value::from(vec![1.5, 2.0]));
    let o = Value::Object(m);
    out.push_str(&format!("{o} {}\n", o == json!({"a": "x", "b": 2, "c": [1.5, 2.0]})));
    let tv = serde_json::to_value(&e).unwrap();
    out.push_str(&format!("{tv}\n"));
    out.push_str(&format!("{}\n", Value::from(f64::NAN)));
    out.push_str(&format!("{:?}\n", Number::from_f64(1.5)));
    from!(out, User,
        json!({"name": "a", "age": 3, "tags": ["x"], "initial": "q"}),
        json!({"name": "a", "age": 300, "tags": [], "initial": "q"}),
        json!({"name": "a", "tags": [], "initial": "q"}),
        json!({"name": "a", "age": 1, "tags": [], "initial": "qq", "zzz": 1}),
        json!({"name": 5}),
        json!(["b", 2, ["t"], null, "c"]),
        json!(["b", 2, ["t"], null, "c", 9]),
        json!({"name": "a", "age": 1, "tags": [], "initial": "q", "extra": {"k": [1, null]}})
    );
    from!(out, Vec<u8>, json!([1, 2]), json!([1, -2]), json!({"a": 1}));
    from!(out, (u8, u8), json!([1, 2]), json!([1, 2, 3]), json!([1]));
    from!(out, BTreeMap<u32, bool>, json!({"1": true, "20": false}), json!({"x": true}), json!({"1 ": true}), json!({"300": true}));
    from!(out, BTreeMap<bool, u8>, json!({"true": 1}), json!({"yes": 1}));
    from!(out, Shape, json!("Dot"), json!({"Circle": 1.5}), json!({"Rect": [1, 2]}), json!({"Rect": []}),
        json!({"Poly": {"sides": 3}}), json!({"Poly": [3]}), json!({"A": 1, "B": 2}), json!(5), json!("Circle"), json!({"Nope": 1}));
    from!(out, Tagged, json!({"type": "Moved", "dx": 4}), json!({"dx": 4}), json!({"type": "Named", "name": "n", "age": 1, "tags": [], "initial": "i"}));
    from!(out, String, json!("s"), json!(1));
    from!(out, f64, json!(1), json!(1.5), json!(-3));
    from!(out, u32, json!(1.5), json!(-1));
    from!(out, Option<u8>, json!(null), json!(7));
    from!(out, Value, json!({"b": [1, 2.5, "x"], "a": null}));
    let user = User { name: "r".into(), age: 9, tags: vec!["t".into()], extra: None, initial: 'z' };
    let value = serde_json::to_value(&user).unwrap();
    out.push_str(&format!("{value}\n"));
    from!(out, User, value);
    let mut v = json!({"b": 1, "a": [true, {"c": null}]});
    out.push_str(&format!("{v:#}\n"));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("d".to_string(), json!("new"));
        obj.remove("b");
    }
    let copy = v.clone();
    if let Some(items) = v["a"].as_array() {
        out.push_str(&format!("{} items\n", items.len()));
    }
    if let Some(obj) = v.as_object_mut() {
        obj.insert("e".to_string(), Value::Bool(false));
    }
    out.push_str(&format!("{v}\n{copy}\n"));
    for (key, value) in v.as_object().unwrap() {
        out.push_str(&format!("{key}={value};"));
    }
    out.push('\n');
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    out.push_str(&format!("{keys:?}\n"));
    let d = Value::default();
    out.push_str(&format!("{d} {} {}\n", d == Value::Null, json!(null) == d));
    let s = Value::from("x");
    out.push_str(&format!("{} {} {} {}\n", s == "x", "x" == s, s == String::from("x"), 3 == json!(3)));
    let name = v.get("d").and_then(|x| x.as_str()).unwrap_or("none");
    out.push_str(&format!("{name}\n"));
    for text in [r#"{"kind":"Data","payload":{"z":[1,2]}}"#, r#"{"payload":"p","kind":"Data"}"#, r#"{"kind":"Empty"}"#] {
        match serde_json::from_str::<Message>(text) {
            Ok(m) => out.push_str(&format!("ok {m:?} -> {}\n", serde_json::to_string(&m).unwrap())),
            Err(e) => out.push_str(&format!("err {e}\n")),
        }
    }
    for text in [r#"{"id":1,"x":true,"y":[null]}"#, r#"{"id":2}"#] {
        match serde_json::from_str::<Open>(text) {
            Ok(o) => out.push_str(&format!("ok {o:?} -> {}\n", serde_json::to_string(&o).unwrap())),
            Err(e) => out.push_str(&format!("err {e}\n")),
        }
    }
    out
}
