import { beforeAll, expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { buildReact, compiler, fixture, run, target } from "./support";
import { decodeMappings, lookup } from "./sourcemap";

beforeAll(buildReact, 600_000);

function compile(source: string, files: Record<string, string> = {}) {
  const dir = fixture("jsx-syntax");
  for (const [name, body] of Object.entries({ "lib.rs": source, ...files })) {
    const path = join(dir, name);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, body);
  }
  const args = [compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js"), "--manifest", join(dir, "manifest.json"),
    "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target];
  return { dir, args };
}

test("JSX supports components across modules, fragments, lists, conditions and spreads", async () => {
  const source = `#![deny(warnings)]
#![allow(non_snake_case)]
#![rust_js::camel_case]
use react::Element;
#[cfg(any())] mod missing;
#[cfg_attr(all(), path = "ui/card.rs")] mod card;
use card::Card as Panel;
pub struct Attrs { pub class_name: &'static str }
pub fn App() -> Element {
    let items: Vec<Element> = (0..3).map(|n| jsx! { <li key={n}>{n}</li> }).collect();
    let attrs = Attrs { class_name: "list" };
    jsx! {
        <>
            <Panel title="Numbers">
                <ul {...attrs}>{items}</ul>
            </Panel>
            {if true { Some(jsx! { <p data-state="ready">{"done"}</p> }) } else { None }}
        </>
    }
}
pub fn Spread() -> Element {
    let props = card::Props { title: "Spread", children: jsx! { <span>{"child"}</span> } };
    jsx! { <Panel {...props} /> }
}
pub fn Override() -> Element {
    let props = card::Props { title: "old", children: jsx! { <span /> } };
    jsx! { <Panel {...card::Props { title: "new", ..props }} /> }
}
`;
  const card = `use react::Element;
pub struct Props { pub title: &'static str, pub children: Element }
pub(crate) fn Card(p: Props) -> Element {
    jsx! { <section><h1>{p.title}</h1>{p.children}</section> }
}
`;
  const { dir, args } = compile(source, { "ui/card.rs": card });
  run(args);
  const code = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(code).toContain('import { Card } from "./card.jsx";');
  expect(code).toContain('<Card title="Numbers">');
  const result = await import(join(dir, "lib.jsx"));
  expect(renderToStaticMarkup(result.App())).toBe('<section><h1>Numbers</h1><ul class="list"><li>0</li><li>1</li><li>2</li></ul></section><p data-state="ready">done</p>');
  expect(renderToStaticMarkup(result.Spread())).toBe('<section><h1>Spread</h1><span>child</span></section>');
  expect(renderToStaticMarkup(result.Override())).toBe('<section><h1>new</h1><span></span></section>');
  const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8"));
  expect(manifest.sources).toContain(join(dir, "ui/card.rs"));
  expect(manifest.sources.some((s: string) => s.includes("jsx expansion"))).toBe(false);
});

test("named component imports avoid local functions, nested parameters and duplicate exports", async () => {
  const source = `#![allow(non_snake_case)]
use react::Element;
mod first;
mod second;
pub fn Card() -> Element { jsx! { <b>{"local"}</b> } }
pub fn View() -> Element {
    let render = |Card: i32| jsx! {
        <>
            <first::Card />
            <second::Card />
            <span>{Card}</span>
        </>
    };
    render(9)
}
`;
  const { dir, args } = compile(source, {
    "first.rs": 'use react::Element; pub fn Card() -> Element { jsx! { <b>{"first"}</b> } }',
    "second.rs": 'use react::Element; pub fn Card() -> Element { jsx! { <b>{"second"}</b> } }',
  });
  run(args);
  const output = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(output).toContain('import { Card as Card$2 } from "./first.jsx";');
  expect(output).toContain('import { Card as Card$3 } from "./second.jsx";');
  expect(output).toContain("<Card$2 />");
  expect(output).toContain("<Card$3 />");
  const result = await import(join(dir, "lib.jsx"));
  expect(renderToStaticMarkup(result.View())).toBe("<b>first</b><b>second</b><span>9</span>");
  expect(renderToStaticMarkup(result.Card())).toBe("<b>local</b>");
  const lines = output.split("\n");
  const map = JSON.parse(readFileSync(join(dir, "lib.jsx.map"), "utf8"));
  for (const [generated, original] of [["Card$2", "<first::Card"], ["Card$3", "<second::Card"]]) {
    const line = lines.findIndex(l => l.includes(`<${generated}`));
    expect(lookup(decodeMappings(map.mappings), line, lines[line].indexOf(generated))?.srcLine)
      .toBe(source.split("\n").findIndex(l => l.includes(original)));
  }
  run(args);
  expect(readFileSync(join(dir, "lib.jsx"), "utf8")).toBe(output);
});

test("JSX preserves evaluation order and maps tags and handler statements to their original lines", async () => {
  const source = `#![allow(non_snake_case)]
use react::Element;
unsafe extern "Rust" {
    #[link_name = "globalThis.record"] safe fn record(n: i32) -> i32;
}
pub fn App() -> Element {
    jsx! {
        <button
            title={record(1).to_string()}
            onClick={move |_| {
                record(5);
                record(6);
            }}
        >
            <span>{record(2)}</span>
            {record(3)}
        </button>
    }
}
`;
  const { dir, args } = compile(source);
  run(args);
  const log: number[] = [];
  const previous = globalThis.record;
  globalThis.record = (n: number) => { log.push(n); return n; };
  try {
    const result = await import(join(dir, "lib.jsx"));
    const tree = result.App();
    expect(log).toEqual([1, 2, 3]);
    tree.props.onClick();
    expect(log).toEqual([1, 2, 3, 5, 6]);
  } finally { globalThis.record = previous; }
  const js = readFileSync(join(dir, "lib.jsx"), "utf8").split("\n");
  const map = JSON.parse(readFileSync(join(dir, "lib.jsx.map"), "utf8"));
  expect(map.sourcesContent).toEqual([source]);
  const segments = decodeMappings(map.mappings);
  for (const [generated, original] of [["button", "<button"], ["globalThis.record(5)", "record(5);"], ["globalThis.record(6)", "record(6);"], ["globalThis.record(2)", "<span>{record(2)}</span>"]]) {
    const line = js.findIndex(l => l.includes(generated));
    const hit = lookup(segments, line, js[line].indexOf(generated));
    expect(hit?.srcLine, generated).toBe(source.split("\n").findIndex(l => l.includes(original)));
  }
});

test("nested component JSX stays readable, contextually typed and mapped to the original Rust", async () => {
  const source = `#![deny(warnings)]
#![allow(non_snake_case)]
#![rust_js::camel_case]
use react::Element;
use std::rc::Rc;
unsafe extern "Rust" { #[link_name = "globalThis.record"] safe fn record(n: i32); }
pub struct Props { pub title: &'static str, pub content: Element, pub on_submit: Option<Rc<dyn Fn()>> }
pub fn Card(p: Props) -> Element { jsx! { <section title={p.title}>{p.content}</section> } }
pub fn App(active: bool) -> Element {
    jsx! {
        <main>
            <Card
                title="Ready"
                content={if active {
                    jsx! { <button onClick={move |_| { record(7); }}>{"Save"}</button> }
                } else {
                    jsx! { <span>{"Waiting"}</span> }
                }}
                onSubmit={Some(Rc::new(move || record(8)))}
            />
        </main>
    }
}
pub fn Tokens() -> Element {
    jsx! { <Card
        title={stringify!(jsx! { untouched })}
        content={
            jsx! { <i /> }
            jsx! { <span /> }
        }
        onSubmit={None}
    /> }
}
`;
  const { dir, args } = compile(source);
  run(args);
  const output = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(output).toContain(`export function App(active) {
  return (
    <main>
      <Card
        title="Ready"
        content={
          active ? <button onClick={() => globalThis.record(7)}>Save</button> : <span>Waiting</span>
        }
        onSubmit={() => globalThis.record(8)}
      />
    </main>
  );
}`);
  const result = await import(join(dir, "lib.jsx"));
  const log: number[] = [];
  const previous = globalThis.record;
  globalThis.record = (n: number) => { log.push(n); };
  try {
    const card = result.App(true).props.children;
    expect(card.type).toBe(result.Card);
    card.props.content.props.onClick();
    card.props.onSubmit();
    expect(log).toEqual([7, 8]);
    expect(renderToStaticMarkup(result.App(false))).toBe('<main><section title="Ready"><span>Waiting</span></section></main>');
    expect(result.Tokens().props.title).toContain("jsx!");
    expect(result.Tokens().props.title).toContain("untouched");
  } finally { globalThis.record = previous; }
  const js = output.split("\n");
  const map = JSON.parse(readFileSync(join(dir, "lib.jsx.map"), "utf8"));
  expect(map.sourcesContent).toEqual([source]);
  const segments = decodeMappings(map.mappings);
  for (const [generated, original] of [["Card", "<Card"], ["button", "<button"], ["globalThis.record(7)", "record(7);"], ["globalThis.record(8)", "record(8)"]]) {
    const line = js.findIndex((l, i) => i > js.findIndex(l => l.includes("export function App")) && l.includes(generated));
    expect(lookup(segments, line, js[line].indexOf(generated))?.srcLine, generated)
      .toBe(source.split("\n").findIndex(l => l.includes(original)));
  }
});

test("JSX children have no twelve-sibling tuple limit", async () => {
  const {dir, args} = compile('use react::Element; pub fn View() -> Element { jsx! { <div>' + Array.from({length: 40}, (_, i) => `<span>{${i}}</span>`).join('') + '</div> } }');
  run(args);
  const result = await import(join(dir, "lib.jsx"));
  expect(result.View().props.children).toHaveLength(40);
});

test("JSX uses SVG's tag and attribute spelling", async () => {
  const { dir, args } = compile(`use react::Element;
pub fn View() -> Element {
    jsx! { <svg viewBox="0 0 10 10"><defs><linearGradient id="paint" /></defs></svg> }
}
`);
  run(args);
  const result = await import(join(dir, "lib.jsx"));
  expect(renderToStaticMarkup(result.View())).toBe('<svg viewBox="0 0 10 10"><defs><linearGradient id="paint"></linearGradient></defs></svg>');
});

test("component props, keys and children evaluate in source order without capturing names", async () => {
  const { dir, args } = compile(`#![allow(non_snake_case)]
use react::Element;
unsafe extern "Rust" { #[link_name = "globalThis.record"] safe fn record(n: i32) -> i32; }
pub struct Props { pub title: i32, pub children: i32 }
pub(crate) fn Card(p: Props) -> Element { jsx! { <div>{p.title}{p.children}</div> } }
pub fn App() -> Element {
    let __jsx0 = 3;
    jsx! { <Card title={record(1)} key={record(2)}>{record(__jsx0)}</Card> }
}
`);
  run(args);
  const log: number[] = [];
  const previous = globalThis.record;
  globalThis.record = (n: number) => { log.push(n); return n; };
  try {
    const result = await import(join(dir, "lib.jsx"));
    const tree = result.App();
    expect(log).toEqual([1, 2, 3]);
    expect(tree.key).toBe("2");
    expect(tree.props).toEqual({ title: 1, children: 3 });
  } finally { globalThis.record = previous; }
});

test("JSX loads nested modules using Rust's directory and cfg rules", async () => {
  const { dir, args } = compile(`use react::Element;
mod outer;
pub fn App() -> Element { outer::view() }
`, {
    "outer.rs": `use react::Element;
mod inner;
mod inline { pub mod leaf; }
#[cfg_attr(all(), path = "alternate.rs")] mod alternate;
pub fn view() -> Element { jsx! { <>{inner::view()}{inline::leaf::view()}{alternate::view()}</> } }
`,
    "outer/inner.rs": 'use react::Element; pub fn view() -> Element { jsx! { <b>{"one"}</b> } }',
    "outer/inline/leaf.rs": 'use react::Element; pub fn view() -> Element { jsx! { <i>{"two"}</i> } }',
    "alternate.rs": 'use react::Element; pub fn view() -> Element { jsx! { <p>{"three"}</p> } }',
  });
  run(args);
  const result = await import(join(dir, "lib.js"));
  expect(renderToStaticMarkup(result.App())).toBe("<b>one</b><i>two</i><p>three</p>");
});

test("a disabled crate or module does not load or expand its contents", () => {
  for (const source of [
    '#![cfg(any())]\nmod missing; pub fn View() { jsx! { invalid } }',
    '#[cfg(any())] mod disabled { mod missing; pub fn view() { jsx! { invalid } } }',
    '#[cfg_attr(all(), path="disabled.rs")] mod disabled;',
  ]) {
    const { args } = compile(source, { "disabled.rs": '#![cfg(any())]\nmod missing;' });
    run(args);
  }
});

for (const [name, body, message] of [
  ["mismatched tag", '<button></div>', 'expected </button>'],
  ["missing closing tag", '<button>', 'missing closing tag'],
  ["invalid event", '<button onClick={123} />', 'expected a'],
  ["invalid prop type", '<button disabled={"wrong"} />', 'expected `bool`'],
  ["duplicate attribute", '<button disabled disabled />', 'duplicate attribute'],
  ["missing component prop", '<Card />', 'missing field'],
  ["unknown component prop", '<Card nope="x" />', 'no field named'],
  ["invalid spread", '<div {...123} />', 'non-struct'],
  ["mixed component spread", '<Card title="x" {...Props { title: "y" }} />', 'either named props or a props spread'],
  ["bare text", '<p>Hello world</p>', 'literal or a Rust expression'],
] as const) {
  test(`JSX ${name} reports the original source and preserves existing output`, () => {
    const {dir, args} = compile(`#![allow(non_snake_case)]\nuse react::Element;\npub struct Props { pub title: &'static str }\npub fn Card(p: Props) -> Element { react::html::div().children(p.title) }\npub fn App() -> Element {\n    jsx! { ${body} }\n}`);
    const output = join(dir, "lib.jsx");
    writeFileSync(output, "previous output");
    const result = Bun.spawnSync(args);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain(message);
    // Missing fields are diagnosed in the hygienic props constructor, with
    // the original invocation as a second label. Other errors point there directly.
    expect(result.stderr.toString()).toContain(name === "missing component prop" ? `6 |     jsx! { ${body} }` : "lib.rs:6:");
    expect(readFileSync(output, "utf8")).toBe("previous output");
    expect(existsSync(join(dir, "manifest.json"))).toBe(false);
  });
}
