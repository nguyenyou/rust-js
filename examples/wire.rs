// Types a server and its client share, written to JSON as serde_json writes
// them (ADR 0077): `rename_all`, `rename`, `skip`, `skip_serializing_if`,
// every enum representation, newtypes, `transparent`, maps and floats. The
// server compiles this with serde; rust-js reads its `#[serde]` attributes
// and writes the same bytes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub order_id: u32,
    pub item_name: String,
    pub qty: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub price: f64,
    pub tags: Vec<String>,
    #[serde(rename = "ok")]
    pub paid: bool,
    #[serde(skip)]
    pub cache: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Dot,
    Circle(f64),
    Rect(f64, f64),
    Poly { sides: u32, name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Started,
    Moved { dx: i32, dy: i32 },
    Placed(Order),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum Msg {
    Ping,
    Text(String),
    Pair(u32, u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Loose {
    Num(f64),
    Word(String),
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Id(pub u32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point(pub i32, pub i32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Meters {
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Everything {
    pub id: Id,
    pub at: Point,
    pub unit: Unit,
    pub len: Meters,
    pub shapes: Vec<Shape>,
    pub events: Vec<Event>,
    pub msgs: Vec<Msg>,
    pub loose: Vec<Loose>,
    pub counts: BTreeMap<String, u32>,
    pub by_id: BTreeMap<u32, bool>,
    pub pair: (u8, char, Option<i8>),
    pub empty: Vec<u32>,
    pub nested: Vec<Vec<u32>>,
    pub text: String,
    pub floats: Vec<f64>,
}

pub fn sample() -> Everything {
    let order = Order {
        order_id: 7,
        item_name: "pen \"blue\"".into(),
        qty: 3,
        note: None,
        price: 2.5,
        tags: vec!["a".into(), "b\n".into()],
        paid: true,
        cache: 99,
    };
    let mut counts = BTreeMap::new();
    counts.insert("x".to_string(), 1);
    counts.insert("a".to_string(), 2);
    let mut by_id = BTreeMap::new();
    by_id.insert(10, true);
    by_id.insert(2, false);
    Everything {
        id: Id(5),
        at: Point(-1, 2),
        unit: Unit,
        len: Meters { value: 1.0 },
        shapes: vec![
            Shape::Dot,
            Shape::Circle(1.5),
            Shape::Rect(2.0, 3.0),
            Shape::Poly {
                sides: 6,
                name: "hex".into(),
            },
        ],
        events: vec![
            Event::Started,
            Event::Moved { dx: 1, dy: -2 },
            Event::Placed(order.clone()),
        ],
        msgs: vec![Msg::Ping, Msg::Text("hi".into()), Msg::Pair(1, 2)],
        loose: vec![Loose::Num(1e21), Loose::Word("w".into()), Loose::Nothing],
        counts,
        by_id,
        pair: (255, 'é', None),
        empty: vec![],
        nested: vec![vec![], vec![1, 2]],
        text: "tab\t\u{1}ü😀".into(),
        floats: vec![0.1, -0.0, 1e-7, 123456789.0, f64::NAN, 1e16],
    }
}

pub fn report() -> String {
    let e = sample();
    let order = Order {
        order_id: 1,
        item_name: "x".into(),
        qty: 1,
        note: Some("gift".into()),
        price: 0.0,
        tags: vec![],
        paid: false,
        cache: 0,
    };
    format!(
        "{}\n{}\n{}\n",
        serde_json::to_string(&e).unwrap(),
        serde_json::to_string_pretty(&e).unwrap(),
        serde_json::to_string(&order).unwrap()
    )
}
