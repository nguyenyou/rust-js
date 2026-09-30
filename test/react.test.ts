import { beforeAll, expect, test } from "bun:test";
import { join } from "node:path";
import { copyFileSync } from "node:fs";
import { root, target, run, buildCompiler, buildReact, compiler } from "./support";

beforeAll(buildCompiler, 600_000);

// ADR 0041: React components, written in Rust with the react crate, are the
// JSX you'd write by hand (ADR 0040), and React runs them.
test("React components are hand-written JSX, and React runs them", () => {
  buildReact();
  const out = join(target, "react-test");
  run([compiler, "test/components.rs", "-o", join(out, "components.js"),
    "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  // A module with JSX is a `.jsx` file.
  const js = require("node:fs").readFileSync(join(out, "components.jsx"), "utf8");
  expect(js).toContain("import {\n  createContext,\n  memo,\n  useContext,\n  useEffect,\n  useId,\n  useMemo,\n  useReducer,\n  useRef,\n  useState,\n} from \"react\";");
  // Props taken apart, as a component takes them; `children` as JSX children.
  expect(js).toContain("export function Card({ title, children }) {\n  return (\n    <div className=\"card\">\n      <h2>{title}</h2>\n      {children}\n    </div>\n  );\n}");
  expect(js).toContain('const [draft, setDraft] = useState("");');
  expect(js).toContain("const left = useMemo(() => todos.filter((t) => !t.done).length, [todos]);");
  // A handler of one call stays in the JSX; one with statements is named first.
  expect(js).toContain("onChange={(e) => setDraft(e.target.value)}\n          onKeyDown={onKeyDown}\n        />");
  const todos = js.slice(js.indexOf("export function Todos()"), js.indexOf("export function Clock()"));
  expect(todos).not.toMatch(/const\s+\S+\s*=\s*\(?\s*</);
  expect(todos).toMatch(/<Card title="Todos">\s*<>\s*<input/);
  expect(todos).toContain("<ul>{items}</ul>");
  expect(js).toContain("const onKeyDown = (e) => {");
  // A list, with its keys.
  expect(js).toContain("return (\n      <li key={t.id} className={t.done ? \"done\" : \"\"} onClick={onClick}>\n        {t.text}\n      </li>");
  expect(js).toContain("<ul>{items}</ul>");
  // `Option::map` to an element: the element, or nothing.
  expect(js).toContain('{t != null ? <p className="latest">{t.text}</p> : undefined}');
  // `()` as an effect's dependencies is `[]`, and its cleanup is a function it returns.
  expect(js).toContain("useEffect(() => {\n    setTicks((t) => (t + 10) | 0);\n    return () => {\n      setTicks(-1);\n    };\n  }, []);");
  // Components by name, as JSX tags.
  expect(js).toContain("<Todos />\n      <Clock />\n      <Themed />");
  // A context and memoized components, made once, as `const`s of the module
  // (from `thread_local!`); a provider is the context as a tag, as in React 19.
  expect(js).toContain('const THEME = createContext("light");\nconst BADGE = memo(Badge);\nconst LOOSE_BADGE = memo(Badge, (a, b) => ');
  expect(js).toContain("const theme = useContext(THEME);");
  expect(js).toContain("<BADGE label=\"outside\" />\n      <THEME value={dark ? \"dark\" : \"light\"}>");
  // `!` of a `&bool`, which rustc writes as `Not::not`.
  expect(js).toContain("setDark((d) => !d)");
  copyFileSync(join(root, "test", "react_app.jsx"), join(out, "react_app.test.jsx"));
  const p = Bun.spawnSync(["bun", "test", "--preload", "./test/happydom.ts", join(out, "react_app.test.jsx")], { cwd: root, stderr: "pipe" });
  const output = p.stdout.toString() + p.stderr.toString();
  expect([p.exitCode, output.match(/(\d+) pass/)?.[1]], output).toEqual([0, "3"]);
}, 60_000);

test("JSX preparation preserves evaluation order, conditional execution and text", async () => {
  const { fixture, compiler } = await import("./support");
  buildReact();
  const dir = fixture("jsx-semantics");
  const input = join(dir, "lib.rs");
  await Bun.write(input, `#![allow(non_snake_case)]
use react::{Element, jsx};
unsafe extern "Rust" {
    #[link_name = "globalThis.record"]
    safe fn record(n: i32) -> i32;
}
pub fn Order() -> Element {
    jsx! {
        <div data-first={record(1).to_string()}>{vec![record(2), record(3), record(4)]}</div>
    }
}
pub fn ChildrenFirst() -> Element {
    let children = vec![record(1), record(2), record(3)];
    jsx! {
        <div title={record(4).to_string()}>{children}</div>
    }
}
pub fn StatementValue() -> Element {
    jsx! {
        <div title={record(1).to_string()}>{{ let n = record(2); vec![n, record(3), record(4)] }}</div>
    }
}
pub fn Conditional(flag: bool) -> Element {
    jsx! {
        <div>{if flag { vec![record(5), record(6), record(7)] } else { vec![record(8)] }}</div>
    }
}
pub fn Text() -> Element {
    jsx! {
        <div
            title="\\\"<&>\\n">
            {" leading <&>{}\\ntrailing "}
        </div>
    }
}
pub fn Capture() -> Element {
    let count = 4;
    jsx! {
        <div onClick={move |_| { record(count); record(count + 1); }} />
    }
}
pub fn Siblings(flag: bool, inner: bool) -> Element {
    let mut n = 1;
    jsx! {
        <div>
            <span title={record(1).to_string()}>{n}</span>
            {if flag { Some(jsx! {
                <b>{if inner { jsx! { <i>{record(2)}</i> } } else { jsx! { <em>{record(3)}</em> } }}</b>
            }) } else { None }}
            {{ n = record(4); n }}
        </div>
    }
}
pub struct Attrs { pub title: String }
unsafe extern "Rust" {
    #[link_name = "globalThis.attributes"]
    safe fn attributes() -> Attrs;
}
pub fn SpreadOrder() -> Element {
    let attrs = attributes();
    jsx! { <div><span {...attrs} />{{ let n = record(9); n }}</div> }
}
pub fn First() -> Element { jsx! { <b>{"first"}</b> } }
pub fn Second() -> Element { jsx! { <i>{"second"}</i> } }
pub fn ComponentOrder() -> Element {
    let mut Selected: fn() -> Element = First;
    jsx! { <div><Selected {...()} />{{ Selected = Second; record(9) }}<Selected {...()} /></div> }
}
`);
  run([compiler, input, "-o", join(dir, "lib.js"), "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  const result = await import(join(dir, "lib.jsx"));
  const log: number[] = [];
  const old = globalThis.record;
  const oldAttributes = globalThis.attributes;
  globalThis.record = (n: number) => { log.push(n); return n; };
  globalThis.attributes = () => ({ get title() { log.push(8); return "snapshot"; } });
  try {
    result.Order();
    expect(log.splice(0)).toEqual([1, 2, 3, 4]);
    result.ChildrenFirst();
    expect(log.splice(0)).toEqual([1, 2, 3, 4]);
    result.StatementValue();
    expect(log.splice(0)).toEqual([1, 2, 3, 4]);
    result.Conditional(false);
    expect(log.splice(0)).toEqual([8]);
    result.Conditional(true);
    expect(log.splice(0)).toEqual([5, 6, 7]);
    expect(result.Text().props).toMatchObject({ title: '"<&>\n', children: " leading <&>{}\ntrailing " });
    const element = result.Capture();
    expect(log).toEqual([]);
    element.props.onClick();
    expect(log).toEqual([4, 5]);
    log.splice(0);
    for (const [flag, inner, expected] of [[false, false, [1, 4]], [true, false, [1, 3, 4]], [true, true, [1, 2, 4]]] as const) {
      const tree = result.Siblings(flag, inner);
      expect(log.splice(0)).toEqual([...expected]);
      expect(tree.props.children[0].props.children).toBe(1);
      expect(tree.props.children[2]).toBe(4);
    }
    const spread = result.SpreadOrder();
    expect(log.splice(0)).toEqual([8, 9]);
    expect(spread.props.children[0].props.title).toBe("snapshot");
    const components = result.ComponentOrder();
    expect(log.splice(0)).toEqual([9]);
    expect(components.props.children[0].type).toBe(result.First);
    expect(components.props.children[2].type).toBe(result.Second);
    const code = await Bun.file(join(dir, "lib.jsx")).text();
    const siblings = code.slice(code.indexOf("export function Siblings("), code.indexOf("export function SpreadOrder("));
    expect(siblings).not.toMatch(/(?:const\s+\S+|\b\w+)\s*=\s*\(?\s*</);
    const map = await Bun.file(join(dir, "lib.jsx.map")).json();
    const { decodeMappings } = await import("./sourcemap");
    expect(map.sourcesContent).toEqual([await Bun.file(input).text()]);
    expect(decodeMappings(map.mappings).length).toBeGreaterThan(10);
  } finally {
    if (old === undefined) delete globalThis.record;
    else globalThis.record = old;
    if (oldAttributes === undefined) delete globalThis.attributes;
    else globalThis.attributes = oldAttributes;
  }
});

// ADR 0043: the rest of React's and React DOM's API, run by React 19.3.
test("React's and React DOM's APIs are hand-written React, and they run", () => {
  buildReact();
  const out = join(target, "react-apis");
  run([compiler, "test/apis.rs", "-o", join(out, "apis.js"),
    "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  const js = require("node:fs").readFileSync(join(out, "apis.jsx"), "utf8");
  // Built-in components are JSX tags, and `use` is `use`.
  expect(js).toContain("return (\n    <Suspense fallback={<p className=\"loading\">Loading</p>}>\n      <Greeting />\n    </Suspense>");
  expect(js).toContain("const text = use(globalThis.greeting);");
  expect(js).toContain('<Activity mode={hidden ? "hidden" : "visible"}>');
  expect(js).toContain("{[1, 2].map((n) => (\n          <Fragment key={n}>");
  // Objects built by methods: a style, raw HTML, and options.
  expect(js).toContain("const style = { color: \"red\", fontSize: 12, \"--gap\": \"4px\" };");
  expect(js).toContain('dangerouslySetInnerHTML={{ __html: "<i>raw</i>" }}');
  expect(js).toContain('return renderToString(<Page />, { identifierPrefix: "s-" });');
  expect(js).toContain('const root = createRoot(container, { identifierPrefix: "c-" });');
  expect(js).toContain('const LAZY_CARD = lazy(() => import("./lazy-card.jsx"));');
  Bun.write(join(out, "lazy-card.jsx"), 'export default function LazyCard() {\n  return <em className="lazy">lazy card</em>;\n}\n');
  copyFileSync(join(root, "test", "apis.jsx"), join(out, "apis.test.jsx"));
  const p = Bun.spawnSync(["bun", "test", "--preload", "./test/happydom.ts", join(out, "apis.test.jsx")], { cwd: root, stderr: "pipe" });
  const output = p.stdout.toString() + p.stderr.toString();
  expect([p.exitCode, output.match(/(\d+) pass/)?.[1]], output).toEqual([0, "7"]);
}, 120_000);

// ADR 0046: with `js::camel_case!()` (ADR 0110), a crate's own functions and
// fields are camelCase in JS too, as its variables already are.
test("a camel_case crate names its functions, fields and props the JS way", async () => {
  const { fixture, compiler } = await import("./support");
  const { mkdirSync, writeFileSync } = await import("node:fs");
  buildReact();
  const dir = fixture("camel-case");
  writeFileSync(join(dir, "lib.rs"), `#![allow(non_snake_case)]
#[rust_js::camel_case]
const _: () = ();

mod people;

use react::{Element, jsx, use_state};

pub fn greet(first_name: &str) -> String {
    people::full_name(&people::make_person(first_name))
}

pub enum Shape {
    Rect { top_left: u32, bottom_right: u32 },
}

pub fn rect_width(shape: &Shape) -> u32 {
    match shape {
        Shape::Rect { top_left, bottom_right } => bottom_right - top_left,
    }
}

pub fn wide_rect() -> Shape {
    Shape::Rect {
        top_left: 1,
        bottom_right: 4,
    }
}

pub fn use_clicks() -> u32 {
    let (clicks, _) = use_state(0u32);
    *clicks
}

pub struct FancyButtonProps {
    pub label_text: String,
    pub on_press: Box<dyn Fn()>,
}

pub fn FancyButton(FancyButtonProps { label_text, on_press }: FancyButtonProps) -> Element {
    jsx! {
        <button onClick={move |_| on_press()}>{label_text}</button>
    }
}

pub fn App() -> Element {
    let clicks = use_clicks();
    jsx! {
        <FancyButton labelText={format!("{clicks} clicks")} onPress={Box::new(|| ())} />
    }
}

#[rust_js::name = "keep_me"]
pub fn keep_me() -> u32 {
    1
}
`);
  writeFileSync(join(dir, "people.rs"), `pub struct Person {
    pub first_name: String,
    pub last_name: String,
    #[rust_js::name = "user_id"]
    pub user_id: u32,
}

pub fn make_person(first_name: &str) -> Person {
    Person { first_name: first_name.to_string(), last_name: "Doe".to_string(), user_id: 7 }
}

pub fn full_name(person: &Person) -> String {
    format!("{} {}", person.first_name, person.last_name)
}
`);
  run([compiler, join(dir, "lib.rs"), "-o", join(dir, "lib.js"), "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  const lib = await Bun.file(join(dir, "lib.jsx")).text();
  const people = await Bun.file(join(dir, "people.js")).text();
  // Functions, across modules.
  expect(lib).toContain('import { fullName, makePerson } from "./people.js";');
  expect(lib).toContain("export function greet(firstName) {\n  return fullName(makePerson(firstName));");
  expect(people).toContain("export function makePerson(firstName) {");
  // Fields: of a struct, of an enum's variant, and one kept by its `#[rust_js::name]`.
  expect(people).toContain("firstName, lastName: \"Doe\", user_id: 7");
  expect(people).toContain('`${person.firstName} ${person.lastName}`');
  expect(lib).toContain("export function rectWidth(shape) {\n  return (shape.bottomRight - shape.topLeft");
  // A hook React finds by its name, and props as React code names them.
  expect(lib).toContain("export function useClicks() {");
  expect(lib).toContain("export function FancyButton({ labelText, onPress }) {");
  expect(lib).toContain("<FancyButton labelText={");
  expect(lib).toContain(" onPress={");
  expect(lib).toContain("export function keep_me() {");
  const module = await import(join(dir, "lib.jsx"));
  expect(module.greet("Ada")).toBe("Ada Doe");
  expect(module.rectWidth(module.wideRect())).toBe(3);
  expect((await import(join(dir, "people.js"))).makePerson("Ada")).toEqual({ firstName: "Ada", lastName: "Doe", user_id: 7 });
});
