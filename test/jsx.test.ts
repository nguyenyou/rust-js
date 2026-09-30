import { beforeAll, expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { buildReact, compiler, expectSnapshot, fixture, root, run, target } from "./support";
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

// Keep the whole emitted module, including imports and formatting. Only the
// temporary source directory varies between runs; never normalize emitted code.
function snapshot(dir: string, name: string) {
  expectSnapshot(dir, join(root, "test/snapshots/jsx", name), {
    normalize: text => text.replaceAll(dir + "/", "test/jsx/"),
  });
}

test("JSX supports components across modules, fragments, lists, conditions and spreads", async () => {
  const source = `#![deny(warnings)]
#![allow(non_snake_case)]
#[rust_js::camel_case]
const _: () = ();
use react::{Element, jsx};
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
  const card = `use react::{Element, jsx};
pub struct Props { pub title: &'static str, pub children: Element }
pub(crate) fn Card(p: Props) -> Element {
    jsx! { <section><h1>{p.title}</h1>{p.children}</section> }
}
`;
  const { dir, args } = compile(source, { "ui/card.rs": card });
  run(args);
  snapshot(dir, "modules");
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

// A package's component (an npm one: the pilot's toasts) is a binding used
// as a JSX tag, and any binding, a function, a method or a getter, can be
// a value, called with just its arguments. Found by the pilot.
test("a component a JS module exports is a JSX tag, and a binding is a value", async () => {
  const source = `#![allow(non_snake_case)]
use react::{Element, jsx};
use react::webapi::{abort_controller, abort_signal};

pub struct BadgeProps {
    pub label: &'static str,
}

#[rust_js::link_name = "./badge.js#Badge"]
pub fn Badge(props: BadgeProps) -> Element {
    unreachable!()
}

unsafe extern "Rust" {
    #[link_name = "encodeURIComponent"]
    safe fn encode(text: &str) -> String;
    #[link_name = "parseInt"]
    safe fn parse_int(text: &str) -> f64;
}

pub fn App() -> Element {
    jsx! { <Badge label="new" /> }
}

pub fn values() -> (Vec<String>, Vec<f64>, Vec<bool>) {
    let encoded = vec!["a b", "c&d"].into_iter().map(encode).collect();
    // As \`.map\`'s own argument, \`parseInt\` would be given each index too.
    let numbers = vec!["10", "10", "10"].into_iter().map(parse_int).collect();
    let controller = abort_controller::new();
    abort_controller::abort(controller);
    let signals = vec![abort_controller::signal(controller), abort_controller::signal(abort_controller::new())];
    let aborted = signals.into_iter().map(abort_signal::aborted).collect();
    (encoded, numbers, aborted)
}
`;
  const { dir, args } = compile(source, { "badge.js": 'import { createElement } from "react"; export function Badge({ label }) { return createElement("b", null, label); }\n' });
  run(args);
  const code = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(code).toContain('import { Badge } from "./badge.js";');
  expect(code).toContain('<Badge label="new" />');
  const result = await import(join(dir, "lib.jsx"));
  expect(renderToStaticMarkup(result.App())).toBe("<b>new</b>");
  expect(result.values()).toEqual([["a%20b", "c%26d"], [10, 10, 10], [true, false]]);
});

test("named component imports avoid local functions, nested parameters and duplicate exports", async () => {
  const source = `#![allow(non_snake_case)]
use react::{Element, jsx};
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
    "first.rs": 'use react::{Element, jsx}; pub fn Card() -> Element { jsx! { <b>{"first"}</b> } }',
    "second.rs": 'use react::{Element, jsx}; pub fn Card() -> Element { jsx! { <b>{"second"}</b> } }',
  });
  run(args);
  snapshot(dir, "import-collisions");
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
use react::{Element, jsx};
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
  snapshot(dir, "evaluation-order");
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
#[rust_js::camel_case]
const _: () = ();
use react::{Element, jsx};
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
  snapshot(dir, "nested-expressions");
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
  const {dir, args} = compile('use react::{Element, jsx}; pub fn View() -> Element { jsx! { <div>' + Array.from({length: 40}, (_, i) => `<span>{${i}}</span>`).join('') + '</div> } }');
  run(args);
  snapshot(dir, "many-children");
  const result = await import(join(dir, "lib.jsx"));
  expect(result.View().props.children).toHaveLength(40);
});

test("JSX uses SVG's tag and attribute spelling", async () => {
  const { dir, args } = compile(`use react::{Element, jsx};
pub fn View() -> Element {
    jsx! { <svg viewBox="0 0 10 10"><defs><linearGradient id="paint" /></defs></svg> }
}
`);
  run(args);
  snapshot(dir, "svg");
  const result = await import(join(dir, "lib.jsx"));
  expect(renderToStaticMarkup(result.View())).toBe('<svg viewBox="0 0 10 10"><defs><linearGradient id="paint"></linearGradient></defs></svg>');
});

test("component props, keys and children evaluate in source order without capturing names", async () => {
  const { dir, args } = compile(`#![allow(non_snake_case)]
use react::{Element, jsx};
unsafe extern "Rust" { #[link_name = "globalThis.record"] safe fn record(n: i32) -> i32; }
pub struct Props { pub title: i32, pub children: i32 }
pub(crate) fn Card(p: Props) -> Element { jsx! { <div>{p.title}{p.children}</div> } }
pub fn App() -> Element {
    let __jsx0 = 3;
    jsx! { <Card title={record(1)} key={record(2)}>{record(__jsx0)}</Card> }
}
`);
  run(args);
  snapshot(dir, "keys");
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
    "outer.rs": `use react::{Element, jsx};
mod inner;
mod inline { pub mod leaf; }
#[cfg_attr(all(), path = "alternate.rs")] mod alternate;
pub fn view() -> Element { jsx! { <>{inner::view()}{inline::leaf::view()}{alternate::view()}</> } }
`,
    "outer/inner.rs": 'use react::{Element, jsx}; pub fn view() -> Element { jsx! { <b>{"one"}</b> } }',
    "outer/inline/leaf.rs": 'use react::{Element, jsx}; pub fn view() -> Element { jsx! { <i>{"two"}</i> } }',
    "alternate.rs": 'use react::{Element, jsx}; pub fn view() -> Element { jsx! { <p>{"three"}</p> } }',
  });
  run(args);
  snapshot(dir, "module-resolution");
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
  ["adjacent roots", '<div /><span />', 'wrap adjacent JSX elements'],
  ["spread followed by attribute", '<div {...Props { title: "x" }} id="y" />', 'put the props spread last'],
  ["multiple spreads", '<div {...Props { title: "x" }} {...Props { title: "y" }} />', 'put the props spread last'],
  ["duplicate children", '<Card title="x" children={1}>{2}</Card>', 'children were provided twice'],
  ["intrinsic generic", '<div::<i32> />', 'generic arguments belong on a function component'],
  ["HTML entity", '<p>&amp;</p>', 'literal or a Rust expression'],
  ["empty attribute expression", '<div title={} />', 'Value'],
  ["bare text", '<p>Hello world</p>', 'literal or a Rust expression'],
] as const) {
  test(`JSX ${name} reports the original source and preserves existing output`, () => {
    const {dir, args} = compile(`#![allow(non_snake_case)]\nuse react::{Element, jsx};\npub struct Props { pub title: &'static str }\npub fn Card(p: Props) -> Element { jsx! { <div>{p.title}</div> } }\npub fn App() -> Element {\n    jsx! { ${body} }\n}`);
    const output = join(dir, "lib.jsx");
    writeFileSync(output, "previous output");
    const result = Bun.spawnSync(args, { cwd: dir });
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain(message);
    // Missing fields are diagnosed in the hygienic props constructor, with
    // the original invocation as a second label. Other errors point there directly.
    expect(result.stderr.toString()).toContain(name === "missing component prop" ? `6 |     jsx! { ${body} }` : "lib.rs:6:");
    expect(readFileSync(output, "utf8")).toBe("previous output");
    expect(existsSync(join(dir, "manifest.json"))).toBe(false);
  });
}

test("JSX covers generic functions, memo/lazy/forward-ref values and context providers", async () => {
  const { dir, args } = compile(`#![deny(warnings)]
#![allow(non_snake_case)]
use react::{Element, Node, jsx};
mod wrapped;
use wrapped::{MEMO as Cached, THEME as Theme};
pub struct Props<T> { pub value: T }
#[cfg_attr(all(), inline)]
pub fn Generic<T: Node>(p: Props<T>) -> Element { jsx! { <span>{p.value}</span> } }
pub struct Children<T> { pub children: T }
pub fn Group<T: Node>(p: Children<T>) -> Element { jsx! { <div>{p.children}</div> } }
pub fn Empty<T>() -> Element { jsx! { <i /> } }
pub fn App() -> Element {
    let Selected = Generic::<i32>;
    jsx! {
        <>
            <Generic value={7} />
            <Generic::<i32> value={8} />
            <Generic::<&'static str> value="borrowed" />
            <Generic::<Vec<i32>> value={vec![9, 10]} />
            <Group::<Vec<i32>>>{vec![1]}</Group>
            <Selected {...Props { value: 11 }} />
            <Empty::<i32> />
            <Cached label="memo" />
            <wrapped::Provider />
            <wrapped::LAZY />
            <wrapped::FORWARD label="forward" />
            <Theme value="dark"><Cached label="child" /></Theme>
            <Theme.Provider value="legacy"><Cached label="old" /></Theme.Provider>
        </>
    }
}
`, {
    "wrapped.rs": `use react::{Context, Element, ForwardRef, Lazy, Memo, Ref, create_context, forward_ref, import_module, jsx, lazy, memo};
pub struct Props { pub label: &'static str }
pub fn Card(p: Props) -> Element { jsx! { <b>{p.label}</b> } }
pub fn Provider() -> Element { jsx! { <i /> } }
pub fn Input(p: Props, _: Ref<Option<i32>>) -> Element { jsx! { <b>{p.label}</b> } }
thread_local! {
    pub static MEMO: Memo<Props> = memo(Card);
    pub static LAZY: Lazy<()> = lazy(|| import_module::<()>("./lazy.jsx"));
    pub static FORWARD: ForwardRef<Props, i32> = forward_ref(Input);
    pub static THEME: Context<&'static str> = create_context("light");
    #[cfg(any())] pub static DISABLED: Memo<Missing> = missing();
}
`,
  });
  run(args);
  snapshot(dir, "component-kinds");
  const code = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(code).toContain('<Generic value={7} />');
  expect(code).toContain('<Generic value={8} />');
  expect(code).toContain('<MEMO label="memo" />');
  expect(code).toContain('<THEME value="dark">');
  expect(code).toContain('<THEME.Provider value="legacy">');
  expect(code).toContain('<FORWARD label="forward" />');
  expect(code).toContain('<LAZY />');
  expect(code).not.toContain('component(');
  const source = readFileSync(join(dir, "lib.rs"), "utf8");
  const map = JSON.parse(readFileSync(join(dir, "lib.jsx.map"), "utf8"));
  const lines = code.split("\n");
  for (const [generated, original] of [["<MEMO label=\"memo\"", "<Cached label=\"memo\""], ["<THEME.Provider", "<Theme.Provider"], ["<Generic value={8}", "<Generic::<i32>"]]) {
    const line = lines.findIndex(l => l.includes(generated));
    expect(lookup(decodeMappings(map.mappings), line, lines[line].indexOf(generated))?.srcLine)
      .toBe(source.split("\n").findIndex(l => l.includes(original)));
  }
});

test("JSX built-ins finish as elements and use one spelling for ref and form actions", () => {
  const { dir, args } = compile(`#![allow(non_snake_case)]
use react::{Element, Style, jsx, use_ref, webapi};
pub fn App() -> Element {
    let object = use_ref(None::<&'static webapi::Element>);
    jsx! {
        <Profiler id="test" onRender={|_, _, _, _, _, _| ()}>
            <ViewTransition name="page">
                <form action={|_: &'static webapi::FormData| ()}>
                    <input ref={object} />
                    <input ref={|_: Option<&'static webapi::Element>| ()} />
                    <button formAction="/save" style={Style::new().color("red")}>{"Save"}</button>
                </form>
            </ViewTransition>
            <Suspense />
            <Activity />
            <StrictMode />
        </Profiler>
    }
}
`);
  run(args);
  snapshot(dir, "builtins");
  const code = readFileSync(join(dir, "lib.jsx"), "utf8");
  for (const name of ["Profiler", "ViewTransition", "Suspense", "Activity", "StrictMode"]) expect(code).toContain(`<${name}`);
  expect(code).toContain('formAction="/save"');
  expect(code).not.toContain('actionFn');
  expect(code).not.toContain('refCallback');
});

for (const [name, body] of [
  ["constructor", "react::html::div()"],
  ["import alias", "{ use react::html::div as make; make() }"],
  ["function value", "{ let make = react::html::div; make() }"],
  ["method", "jsx! { <div /> }.children(\"no\")"],
  ["UFCS", "react::Element::children(jsx! { <div /> }, \"no\")"],
  ["component", "react::component(Card, ())"],
  ["fragment", "react::fragment(())"],
  ["inside JSX expression", "jsx! { <div>{react::html::span()}</div> }"],
  ["inside component prop", "jsx! { <Wrapper content={react::html::span()} /> }"],
  ["inside closure", "jsx! { <button onClick={|_| { let _ = react::html::span(); }} /> }"],
  ["ordinary macro", "{ macro_rules! old { () => { react::html::span() } } old!() }"],
] as const) {
  test(`direct element builder ${name} is rejected without replacing output`, () => {
    const { dir, args } = compile(`#![allow(non_snake_case, dead_code)]
use react::{Element, jsx};
pub fn Card() -> Element { jsx! { <div /> } }
pub struct Props { pub content: Element }
pub fn Wrapper(p: Props) -> Element { p.content }
fn unused() -> Element { ${body} }
`);
    const file = join(dir, "lib.jsx");
    writeFileSync(file, "previous output");
    const result = Bun.spawnSync(args);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain("element builders are compiler-only");
    expect(readFileSync(file, "utf8")).toBe("previous output");
  });
}

test("JSX context providers and refs work on React 18 while newer APIs stay gated", () => {
  const { dir, args } = compile(`#![deny(warnings)]
#![allow(non_snake_case)]
use react::{Context, Element, create_context, jsx, webapi};
thread_local! { static THEME: Context<&'static str> = create_context("light"); }
pub fn App() -> Element {
    jsx! {
        <THEME.Provider value="dark">
            <Suspense key="body" fallback="loading">
                <form action="/save"><input ref={|_: Option<&'static webapi::Element>| ()} /></form>
            </Suspense>
        </THEME.Provider>
    }
}
`);
  const source = readFileSync(join(dir, "lib.rs"), "utf8");
  const metadata = join(dir, "libreact.rmeta");
  run(["react/build.sh", "-o", metadata, "--react", "18.2.0"]);
  const versionArgs = args.map(arg => arg === `react=${join(target, "libreact.rmeta")}` ? `react=${metadata}` : arg === target ? dir : arg);
  run(versionArgs);
  snapshot(dir, "react18");
  const code = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(code).toContain('<THEME.Provider value="dark">');
  expect(code).toContain('<Suspense key="body" fallback="loading">');
  for (const unsupported of [
    source.replaceAll("THEME.Provider", "THEME"),
    source.replace('action="/save"', "action={|_: &'static webapi::FormData| ()}"),
    source.replace('key="body" fallback="loading"', '').replaceAll('Suspense', 'Activity'),
  ]) {
    writeFileSync(join(dir, "lib.rs"), unsupported);
    expect(Bun.spawnSync(versionArgs, { cwd: dir }).exitCode).not.toBe(0);
    expect(readFileSync(join(dir, "lib.jsx"), "utf8")).toBe(code);
  }
}, 30_000);

test("forwarded refs keep their handle type, evaluation order and handwritten JSX", async () => {
  const { dir, args } = compile(`#![allow(non_snake_case)]
use react::{Element, ForwardRef, Ref, forward_ref, jsx, webapi};
unsafe extern "Rust" { #[link_name = "globalThis.record"] safe fn record(n: i32) -> i32; }
pub struct Props { pub label: i32 }
pub fn Input(p: Props, reference: Ref<Option<&'static webapi::Element>>) -> Element {
    jsx! { <input ref={reference} title={p.label} /> }
}
thread_local! { static INPUT: ForwardRef<Props, &'static webapi::Element> = forward_ref(Input); }
pub fn Plain(reference: Ref<Option<&'static webapi::Element>>) -> Element {
    jsx! { <INPUT ref={reference} label={1} /> }
}
pub fn App(reference: Ref<Option<&'static webapi::Element>>) -> Element {
    jsx! { <INPUT ref={record(1); reference} label={record(2)} /> }
}
pub struct NormalProps { pub r#ref: Ref<Option<&'static webapi::Element>>, pub title: i32 }
pub fn Normal(p: NormalProps) -> Element { jsx! { <input ref={p.r#ref} title={p.title} /> } }
pub fn Ordinary(reference: Ref<Option<&'static webapi::Element>>) -> Element {
    jsx! { <Normal title={record(3)} ref={record(4); reference} /> }
}
`);
  run(args);
  snapshot(dir, "refs");
  const output = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(output).not.toContain('checked_ref');
  expect(output).not.toContain('component(');
  expect(output).toContain('export function Plain(reference) {\n  return <INPUT label={1} ref={reference} />;\n}');
  const result = await import(join(dir, "lib.jsx"));
  const previous = globalThis.record;
  const calls: number[] = [];
  globalThis.record = (n: number) => { calls.push(n); return n; };
  try {
    const reference = { current: null };
    const tree = result.App(reference);
    expect(calls).toEqual([1, 2]);
    expect(tree.props.ref).toBe(reference);
    expect(tree.type.render(tree.props, tree.props.ref).props.ref).toBe(reference);
    result.Ordinary(reference);
    expect(calls).toEqual([1, 2, 3, 4]);
  } finally { globalThis.record = previous; }
  const file = join(dir, "lib.rs");
  const source = readFileSync(file, "utf8");
  writeFileSync(file, source.replace('pub fn App(reference: Ref<Option<&\'static webapi::Element>>)', 'pub fn App(reference: Ref<Option<i32>>)'));
  const invalid = Bun.spawnSync(args, { cwd: dir });
  expect(invalid.exitCode).not.toBe(0);
  expect(invalid.stderr.toString()).toContain('RefValue');
  expect(readFileSync(join(dir, "lib.jsx"), "utf8")).toBe(output);
});

// Grammar cases live together so their complete output is easy to review.
// API-specific behavior (hooks, mounting, async actions) stays in react.test.ts.
test("JSX grammar: literals, empty forms, Rust children and attribute expressions", async () => {
  const { dir, args } = compile(`#![allow(non_snake_case)]
use react::{Element, Style, inner_html, jsx};
pub fn Empty() -> Element { jsx! { <></> } }
pub fn Literals() -> Element {
    jsx! {
        <div>
            " leading "
            {"<&>"}
            {r#"raw "quote""#}
            {42}{1.5}{true}{false}{()}{None::<i32>}{Some(7)}
            {/* a comment contributes no child */}
            <span></span>
        </div>
    }
}
pub fn Expressions(show: bool) -> Element {
    let pair = (1, 2);
    let list = vec![3, 4];
    jsx! {
        <section>
            {pair}{list}
            {if show { Some(jsx! { <b>{"yes"}</b> }) } else { None }}
            {match show { true => "on", false => "off" }}
            {let n = 5; n + 1}
        </section>
    }
}
pub fn Attributes() -> Element {
    jsx! {
        <>
            <button disabled title={let n = 2; n.to_string()} aria-label="Save" data-state="ready" tabIndex=3 />
            <input disabled={false} defaultValue="a" />
            <div style={Style::new().color("red")} dangerouslySetInnerHTML={inner_html("<b>raw</b>")} />
        </>
    }
}
`);
  run(args);
  snapshot(dir, "grammar");
  const source = readFileSync(join(dir, "lib.rs"), "utf8").split("\n");
  const lines = readFileSync(join(dir, "lib.jsx"), "utf8").split("\n");
  const map = JSON.parse(readFileSync(join(dir, "lib.jsx.map"), "utf8"));
  for (const [generated, original] of [["const n = 5", "{let n = 5"], ["const n = 2", "title={let n = 2"]]) {
    const line = lines.findIndex(l => l.includes(generated));
    expect(line).toBeGreaterThanOrEqual(0);
    expect(lookup(decodeMappings(map.mappings), line, lines[line].indexOf("n ="))?.srcLine)
      .toBe(source.findIndex(l => l.includes(original)));
  }
  const result = await import(join(dir, "lib.jsx"));
  expect(renderToStaticMarkup(result.Empty())).toBe("");
  expect(renderToStaticMarkup(result.Literals())).toBe('<div> leading &lt;&amp;&gt;raw &quot;quote&quot;421.57<span></span></div>');
  expect(renderToStaticMarkup(result.Expressions(true))).toBe('<section>1234<b>yes</b>on6</section>');
  expect(renderToStaticMarkup(result.Expressions(false))).toBe('<section>1234off6</section>');
  expect(renderToStaticMarkup(result.Attributes())).toBe('<button disabled="" title="2" aria-label="Save" data-state="ready" tabindex="3"></button><input value="a"/><div style="color:red"><b>raw</b></div>');
});

test("JSX grammar: spread precedence, children overrides, component paths and keyed fragments", async () => {
  const { dir, args } = compile(`#![allow(non_snake_case)]
#[rust_js::camel_case]
const _: () = ();
use react::{Element, jsx};
mod ui;
use ui::Card as Panel;
pub struct Attrs { pub title: &'static str, pub class_name: &'static str }
pub fn App() -> Element {
    let attrs = Attrs { title: "spread", class_name: "card" };
    let props = ui::Props { title: "panel", children: jsx! { <i>{"old"}</i> } };
    let Selected = ui::Empty;
    jsx! {
        <Fragment key="group">
            <div title="named" {...attrs} />
            <Panel {...props}><b>{"new"}</b></Panel>
            <ui.Card title="dot"><i /></ui.Card>
            <ui::Card title="path" children={jsx! { <u /> }} />
            <Selected {...()} />
        </Fragment>
    }
}
`, {
    "ui.rs": `use react::{Element, jsx};
pub struct Props { pub title: &'static str, pub children: Element }
pub fn Card(p: Props) -> Element { jsx! { <section title={p.title}>{p.children}</section> } }
pub fn Empty() -> Element { jsx! { <hr /> } }
`,
  });
  run(args);
  snapshot(dir, "spreads-and-paths");
  const result = await import(join(dir, "lib.jsx"));
  const tree = result.App();
  expect(tree.key).toBe("group");
  expect(renderToStaticMarkup(tree)).toBe('<div title="spread" class="card"></div><section title="panel"><b>new</b></section><section title="dot"><i></i></section><section title="path"><u></u></section><hr/>');
});
