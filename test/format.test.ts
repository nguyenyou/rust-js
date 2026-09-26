import { beforeAll, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildReact, compiler, fixture, root, run, target } from "./support";
import { decodeMappings, lookup } from "./sourcemap";

beforeAll(buildReact, 600_000);

function format(source: string) {
  return Bun.spawnSync([compiler, "--format-jsx"], { stdin: Buffer.from(source), stderr: "pipe" });
}

test("JSX formatter aligns nested props and callbacks and is idempotent", () => {
  const expected = `fn view() {
    jsx! {
        <Pane
            editor={jsx! {
                <Editor
                    state={current}
                    onSubmit={Some(Rc::new(move || {
                        submit(false);
                    }))}
                />
            }}
        />
    }
}
`;
  const input = expected.replace(/^ +/gm, " ");
  // The JSX pass takes the surrounding Rust indentation as its baseline.
  const result = format(input.replace(" jsx!", "    jsx!"));
  expect(result.exitCode, result.stderr.toString()).toBe(0);
  expect(result.stdout.toString()).toBe(expected);
  expect(format(expected).stdout.toString()).toBe(expected);
});

test("JSX formatter preserves literal contents, comments, other macros and skipped items", () => {
  const preserved = [
    'r#"first\n  second\nlast"#',
    '"first\n second"',
    'stringify!(\n jsx! { untouched }\n)',
    'foreign::jsx!(\n not our grammar\n)',
    '/* first\n second */',
    'macro_rules! local {\n () => { opaque };\n}',
  ];
  const skipped = `#[rustfmt::skip]
fn skipped() { jsx! {
 <p />
} }
`;
  const skippedCall = `#[rustfmt::skip]
    jsx! {
 <p />
    };`;
  const source = `fn view<'a>(s: &'a str) {
    jsx! { <div title={${preserved[0]}}>
{${preserved[1]}}
{${preserved[2]}}
{${preserved[3]}}
{${preserved[4]}}
{${preserved[5]} if s.len() < 3 { s } else { "long" }}
    </div> }
}
${skipped}`;
  const result = format(source);
  expect(result.exitCode, result.stderr.toString()).toBe(0);
  const output = result.stdout.toString();
  for (const text of [...preserved, skipped]) expect(output).toContain(text);
  expect(format(output).stdout.toString()).toBe(output);
  expect(format("// plain Rust\nfn f() {}\n").stdout.toString()).toBe("// plain Rust\nfn f() {}\n");
  expect(format("").exitCode).toBe(0);
  const statement = `fn view() {\n    ${skippedCall}\n}\n`;
  expect(format(statement).stdout.toString()).toBe(statement);
  const method = `impl View {\n${skipped}}\n`;
  expect(format(method).stdout.toString()).toBe(method);
});

test("formatter check is read-only and invalid input prevents selected-file writes", () => {
  const dir = fixture("format");
  const file = join(dir, "input.rs");
  const bad = join(dir, "invalid.rs");
  const source = "fn view() { jsx! {\n <div />\n} }\n";
  writeFileSync(file, source);
  const command = ["bun", join(root, "scripts/format.ts")];
  expect(Bun.spawnSync([...command, "--check", file]).exitCode).toBe(1);
  expect(readFileSync(file, "utf8")).toBe(source);
  for (const invalid of ["fn broken( {", "fn view() { jsx! { <div></p> } }"]) {
    writeFileSync(bad, invalid);
    expect(Bun.spawnSync([...command, file, bad]).exitCode).not.toBe(0);
    expect(readFileSync(file, "utf8")).toBe(source);
    expect(readFileSync(bad, "utf8")).toBe(invalid);
    const result = format(invalid);
    expect(result.exitCode).not.toBe(0);
    expect(result.stdout.toString()).toBe("");
  }
  run([...command, file]);
  expect(readFileSync(file, "utf8")).toBe("fn view() {\n    jsx! {\n        <div />\n    }\n}\n");
  run([...command, "--check", file]);
}, 30_000);

test("formatting preserves generated JSX and maps handlers to the formatted source", () => {
  const dir = fixture("format-map");
  const file = join(dir, "lib.rs");
  const source = `use react::Element;
unsafe extern "Rust" { #[link_name = "globalThis.record"] safe fn record(n: i32); }
pub fn view() -> Element {
    jsx! {
 <button
 onClick={move |_| {
 record(7);
 record(8);
 }}
 >{"Save"}</button>
    }
}
`;
  const command = [compiler, file, "--", "--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target];
  writeFileSync(file, source);
  run(command);
  const before = readFileSync(join(dir, "lib.jsx"), "utf8");
  run(["bun", join(root, "scripts/format.ts"), file]);
  run(command);
  const after = readFileSync(join(dir, "lib.jsx"), "utf8");
  expect(after).toBe(before);
  const formatted = readFileSync(file, "utf8");
  const map = JSON.parse(readFileSync(join(dir, "lib.jsx.map"), "utf8"));
  expect(map.sourcesContent).toEqual([formatted]);
  const lines = after.split("\n");
  const line = lines.findIndex(l => l.includes("globalThis.record(7)"));
  expect(lookup(decodeMappings(map.mappings), line, lines[line].indexOf("globalThis.record(7)"))?.srcLine)
    .toBe(formatted.split("\n").findIndex(l => l.includes("record(7);")));
});
