import { beforeAll, expect, test } from "bun:test";
import { join } from "node:path";
import { copyFileSync } from "node:fs";
import { root, target, run, buildCompiler, buildReact } from "./support";

beforeAll(buildCompiler, 600_000);

// ADR 0041: React components, written in Rust with the react crate, are the
// JSX you'd write by hand (ADR 0040), and React runs them.
test("React components are hand-written JSX, and React runs them", () => {
  buildReact();
  const out = join(target, "react-test");
  run([join(target, "debug", "rust-js"), "test/components.rs", "-o", join(out, "components.js"),
    "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  // A module with JSX is a `.jsx` file.
  const js = require("node:fs").readFileSync(join(out, "components.jsx"), "utf8");
  expect(js).toContain('import { createContext, memo, useContext, useEffect, useId, useMemo, useReducer, useRef, useState } from "react";');
  // Props taken apart, as a component takes them; `children` as JSX children.
  expect(js).toContain("export function Card({ title, children }) {\n  return <div className=\"card\">\n    <h2>{title}</h2>\n    {children}\n  </div>;\n}");
  expect(js).toContain('const [draft, setDraft] = useState("");');
  expect(js).toContain("const left = useMemo(() => todos.filter((t) => !t.done).length, [todos]);");
  // A handler of one call stays in the JSX; one with statements is named first.
  expect(js).toContain("onChange={(e) => setDraft(e.target.value)} onKeyDown={onKeyDown} />");
  expect(js).toContain("const onKeyDown = (e) => {");
  // A list, with its keys.
  expect(js).toContain('return <li key={t.id} className={t.done ? "done" : ""} onClick={onClick}>{t.text}</li>;');
  expect(js).toContain("<ul>{items}</ul>");
  // `()` as an effect's dependencies is `[]`, and its cleanup is a function it returns.
  expect(js).toContain("useEffect(() => {\n    setTicks((t) => t + 10 | 0);\n    return () => {\n      setTicks(-1);\n    };\n  }, []);");
  // Components by name, as JSX tags.
  expect(js).toContain("<Todos />\n    <Clock />\n    <Themed />");
  // A context and memoized components, made once, as `const`s of the module
  // (from `thread_local!`); a provider is the context as a tag, as in React 19.
  expect(js).toContain('const THEME = createContext("light");\nconst BADGE = memo(Badge);\nconst LOOSE_BADGE = memo(Badge, (a, b) => ');
  expect(js).toContain("const theme = useContext(THEME);");
  expect(js).toContain('<BADGE label="outside" />\n    <THEME value={dark ? "dark" : "light"}>');
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
use react::{Element, html::div};
unsafe extern "Rust" {
    #[link_name = "globalThis.record"] safe fn record(n: i32) -> i32;
}
pub fn Order() -> Element {
    div().attr("data-first", record(1).to_string()).children(vec![record(2), record(3), record(4)])
}
pub fn ChildrenFirst() -> Element {
    div().children(vec![record(1), record(2), record(3)]).attr("title", record(4).to_string())
}
pub fn StatementValue() -> Element {
    div().attr("title", record(1).to_string()).children({ let n = record(2); vec![n, record(3), record(4)] })
}
pub fn Conditional(flag: bool) -> Element {
    div().children(if flag { vec![record(5), record(6), record(7)] } else { vec![record(8)] })
}
pub fn Text() -> Element {
    div().attr("title", "\\\"<&>\\n").children(" leading <&>{}\\ntrailing ")
}
pub fn Capture() -> Element {
    let count = 4;
    div().on_click(move |_| { record(count); record(count + 1); })
}
`);
  run([compiler, input, "-o", join(dir, "lib.js"), "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target]);
  const result = await import(join(dir, "lib.jsx"));
  const log: number[] = [];
  const old = globalThis.record;
  globalThis.record = (n: number) => { log.push(n); return n; };
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
    const map = await Bun.file(join(dir, "lib.jsx.map")).json();
    const { decodeMappings } = await import("./sourcemap");
    expect(map.sourcesContent).toEqual([await Bun.file(input).text()]);
    expect(decodeMappings(map.mappings).length).toBeGreaterThan(10);
  } finally {
    if (old === undefined) delete globalThis.record;
    else globalThis.record = old;
  }
});
