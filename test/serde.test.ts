// Differential regressions: native serde_json is the oracle, using exactly
// the same Rust source and dependency versions as the generated JavaScript.
import { beforeAll, expect, test } from "bun:test";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCompiler, buildSerde, compiler, fixture, run } from "./support";

beforeAll(() => {
  buildCompiler();
  buildSerde("rlib");
}, 600_000);

const cases = [
  {
    name: "ordinary renamed structs and optional fields agree (control)",
    definitions: `#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub record_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}`,
    value: "Record { record_id: 7, note: None }",
  },
  {
    name: "tuple struct skips do not disclose omitted fields",
    definitions: `#[derive(serde::Serialize)]
pub struct Credentials(pub u32, #[serde(skip_serializing)] pub String, pub u32);`,
    value: 'Credentials(1, "secret".into(), 2)',
  },
  {
    name: "tuple variant skips do not disclose omitted fields",
    definitions: `#[derive(serde::Serialize)]
pub enum Message { Credentials(u32, #[serde(skip)] String, u32) }`,
    value: 'Message::Credentials(1, "secret".into(), 2)',
  },
  {
    name: "conditional skips apply to tuple fields",
    definitions: `#[derive(serde::Serialize)]
pub struct Values(#[serde(skip_serializing_if = "Option::is_none")] pub Option<u32>, pub u32);`,
    value: 'Values(None, 2)',
  },
  {
    name: "an untagged variant overrides its enum representation",
    definitions: `#[derive(serde::Serialize)]
pub enum Message { Named(u32), #[serde(untagged)] Other(u32) }`,
    value: 'Message::Other(7)',
  },
  {
    name: "a tagged struct includes its renamed type tag",
    definitions: `#[derive(serde::Serialize)]
#[serde(tag = "kind", rename = "record")]
pub struct Record { pub id: u32 }`,
    value: 'Record { id: 7 }',
  },
  {
    name: "internal tags respect a transparent newtype payload",
    definitions: `#[derive(serde::Serialize)]
pub struct Details { pub id: u32 }
#[derive(serde::Serialize)]
#[serde(transparent)]
pub struct Wrapper { pub value: Details }
#[derive(serde::Serialize)]
#[serde(tag = "kind")]
pub enum Message { Data(Wrapper) }`,
    value: 'Message::Data(Wrapper { value: Details { id: 7 } })',
  },
  {
    name: "skip predicates resolve in their declaring module",
    definitions: `pub mod first {
    pub fn skip(_: &u32) -> bool { true }
    #[derive(serde::Serialize)]
    pub struct Record { #[serde(skip_serializing_if = "skip")] pub id: u32 }
}
pub mod second {
    pub fn skip(_: &u32) -> bool { false }
    #[derive(serde::Serialize)]
    pub struct Record { #[serde(skip_serializing_if = "skip")] pub id: u32 }
}`,
    value: '(first::Record { id: 1 }, second::Record { id: 2 })',
  },
  {
    name: "tuple skips preserve included values and empty arrays",
    definitions: `#[derive(serde::Serialize)]
pub struct Hidden(#[serde(skip)] pub String, #[serde(skip)] pub u32);
#[derive(serde::Serialize)]
pub enum Message {
    Values(#[serde(skip_serializing_if = "Option::is_none")] Option<u32>, u32),
}`,
    value: '(Hidden("secret".into(), 1), Message::Values(None, 2), Message::Values(Some(3), 4))',
  },
  {
    name: "skip predicates honor imports and fully qualified paths",
    definitions: `pub mod helpers { pub fn skip(_: &u32) -> bool { true } }
use helpers::skip as omit;
#[derive(serde::Serialize)]
pub struct Record {
    #[serde(skip_serializing_if = "omit")] pub hidden: u32,
    #[serde(skip_serializing_if = "crate::helpers::skip")] pub qualified: u32,
    #[serde(skip_serializing_if = "std::option::Option::is_none")] pub absent: Option<u32>,
}`,
    value: 'Record { hidden: 1, qualified: 2, absent: None }',
  },
  {
    name: "a user predicate named Option::is_none is not a standard-library intrinsic",
    definitions: `pub struct Option;
impl Option { pub fn is_none(value: &std::option::Option<u32>) -> bool { value.is_some() } }
#[derive(serde::Serialize)]
pub struct Record { #[serde(skip_serializing_if = "Option::is_none")] pub hidden: std::option::Option<u32> }`,
    value: '(Record { hidden: None }, Record { hidden: Some(1) })',
  },
  {
    name: "internal tags unwrap nested transparent and newtype payloads",
    definitions: `#[derive(serde::Serialize)]
#[serde(tag = "type", rename = "details")]
pub struct Details { pub id: u32 }
#[derive(serde::Serialize)]
pub struct Newtype(pub Details);
#[derive(serde::Serialize)]
#[serde(transparent)]
pub struct Wrapper { #[serde(skip)] pub hidden: u32, pub value: Newtype }
#[derive(serde::Serialize)]
#[serde(tag = "kind")]
pub enum Message { Data(Wrapper) }`,
    value: 'Message::Data(Wrapper { hidden: 9, value: Newtype(Details { id: 7 }) })',
  },
] as const;

for (const { name, definitions, value } of cases) {
  test(`serde: ${name}`, async () => {
    const dir = fixture("serde-regression");
    writeFileSync(join(dir, "cases.rs"), `${definitions}
pub fn report() -> String {
    let value = ${value};
    format!("{}\\n{}", serde_json::to_string(&value).unwrap(), serde_json::to_string_pretty(&value).unwrap())
}
`);
    writeFileSync(join(dir, "native.rs"), `include!("cases.rs");
fn main() { println!("{}", serde_json::to_string(&report()).unwrap()); }
`);
    run(["rustc", "--edition=2024", "-Awarnings", join(dir, "native.rs"), "-o", join(dir, "native"), ...buildSerde("rlib")]);
    const expected = JSON.parse(run([join(dir, "native")]));
    run([compiler, join(dir, "cases.rs"), "-o", join(dir, "cases.js"), "--", ...buildSerde()]);
    const generated = await import(join(dir, "cases.js"));
    expect(generated.report()).toBe(expected);
  });
}
