// The corpus: Rust programs, each a `fn main()` as rustc's own tests are,
// run natively and as JS under Bun and Node, which must print the same and
// end the same (ADR 0088). What a case expects is in its `//@` directives:
//
//   //@ run-pass                   main returns (the default)
//   //@ run-fail: <message>        main panics with exactly this message (\n for a newline)
//   //@ compile-fail: <text>       rust-js rejects it, with this in its first error
//   //@ ignore-rust-js: <reason>   rust-js gets it wrong for now; passing is an error
//
//   case.rs ─┬─ rustc ──► native ─────────────────┐
//            └─ rust-js ──► case.js ─┬─ bun  ──────┼─► stdout, stderr, outcome: the same?
//                                    └─ node ──────┘

import { beforeAll, expect, test } from "bun:test";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { runInNewContext } from "node:vm";
import { build } from "vite";

import { expected, same, type Outcome } from "./oracle";
import { agree, compileJs, node, runJs, runNative, runtimes, show, type Run } from "./programs";
import { buildCompiler, compiler, fixture, root } from "./support";

const corpus = join(root, "test/corpus");

type Expect =
  | { kind: "run-pass" }
  | { kind: "run-fail"; message: string }
  | { kind: "compile-fail"; text: string }
  | { kind: "ignore-rust-js"; reason: string };

/** What a case's directives say, or the problems with them. */
function directives(source: string): Expect | string {
  const found: Expect[] = [];
  for (const [, line] of source.matchAll(/^\/\/@(.*)$/gm)) {
    const [, name, value] = /^ ([a-z-]+)(?:: (.+))?$/.exec(line) ?? [];
    if (name === "run-pass" && value === undefined) found.push({ kind: "run-pass" });
    else if (name === "run-fail" && value) found.push({ kind: "run-fail", message: value.replaceAll("\\n", "\n") });
    else if (name === "compile-fail" && value) found.push({ kind: "compile-fail", text: value });
    else if (name === "ignore-rust-js" && value) found.push({ kind: "ignore-rust-js", reason: value });
    else return `unknown or malformed directive \`//@${line}\``;
  }
  if (found.length > 1) return "more than one directive";
  return found[0] ?? { kind: "run-pass" };
}


/** The case's JS as a production build ships it: one file, bundled and
 * minified by Vite, with Rolldown and Oxc, as an app's is. */
async function bundle(js: string, dir: string): Promise<string> {
  const outDir = join(dir, "production");
  await build({
    configFile: false,
    logLevel: "silent",
    root: dir,
    build: {
      outDir,
      emptyOutDir: true,
      minify: true,
      target: "esnext",
      lib: { entry: js, formats: ["es"], fileName: () => "case.min.js" },
    },
  });
  return join(outDir, "case.min.js");
}

/** What's wrong with a case: nothing, if native Rust does what its
 * directives say and the JS does what native Rust does, as it's compiled
 * and as it's shipped. */
async function check(file: string): Promise<string[]> {
  const want = directives(readFileSync(file, "utf8"));
  if (typeof want === "string") return [want];
  const dir = fixture(`corpus-${basename(file, ".rs")}`);
  const native = runNative(file, dir);
  if (typeof native === "string") return [native];

  // The directive is checked against Rust itself, so it can't be wrong.
  const nativeOutcome: Outcome = want.kind === "run-fail" ? expected({ panic: want.message }) : { value: null };
  if (typeof native.outcome === "string" || !same(native.outcome, nativeOutcome)) {
    return [`native Rust ended ${show(native.outcome)}, but the directive says ${show(nativeOutcome)}`];
  }

  const compiled = compileJs(file, dir);
  if (want.kind === "compile-fail") {
    if ("js" in compiled) return ["rust-js compiled it, but `compile-fail` says it can't"];
    // A rejection, not a crash, which may begin with the rejection it expects.
    if (compiled.kind === "crashed") return [`${compiled.error}\n\n\`compile-fail\` expects a rejection`];
    // The first error, so an error of rustc's own can't hide before rust-js's.
    const first = compiled.error.split("\n").find((line) => line.startsWith("error")) ?? "";
    return first.includes(want.text) ? [] : [`rust-js's first error doesn't say \`${want.text}\`:\n${compiled.error}`];
  }
  const problems: string[] = [];
  const runs: [string, Run][] = [];
  if ("js" in compiled) {
    for (const [name, cmd] of runtimes) runs.push([name, runJs(cmd, compiled.js, dir, name)]);
    runs.push(["node, minified", runJs([node ?? "node"], await bundle(compiled.js, dir), dir, "minified")]);
  }
  if (want.kind === "ignore-rust-js") {
    // Wrong for now, rejected or answered otherwise, but not a crash.
    if ("error" in compiled && compiled.kind === "crashed") return [compiled.error];
    const passes = runs.length > 0 && runs.every(([, run]) => agree(run, native));
    return passes ? [`it passes now: remove \`ignore-rust-js: ${want.reason}\``] : [];
  }
  if ("error" in compiled) return [`rust-js can't compile it:\n${compiled.error}`];
  for (const [name, run] of runs) {
    if (run.stdout !== native.stdout) problems.push(`${name} stdout:\n${run.stdout}\nnative stdout:\n${native.stdout}`);
    if (run.stderr !== native.stderr) problems.push(`${name} stderr:\n${run.stderr}\nnative stderr:\n${native.stderr}`);
    if (typeof run.outcome === "string" || !same(run.outcome, native.outcome as Outcome)) {
      problems.push(`${name} ended ${show(run.outcome)}, native Rust ${show(native.outcome)}`);
    }
  }
  return problems;
}

beforeAll(() => {
  buildCompiler();
  if (!node) throw new Error("the corpus runs under Node too: install Node 22.18 or later");
});

const cases = readdirSync(corpus).filter((f) => f.endsWith(".rs")).sort();

test("the corpus has cases", () => {
  expect(cases.length).toBeGreaterThan(0);
});

for (const name of cases) {
  test(name, async () => {
    expect(await check(join(corpus, name))).toEqual([]);
  }, 120_000);
}

// Negative controls: each of these cases is wrong, and must be reported.
function control(name: string, source: string): Promise<string[]> {
  const file = join(fixture("corpus-control"), `${name}.rs`);
  writeFileSync(file, source);
  return check(file);
}

test("a directive native Rust disagrees with is reported", async () => {
  const problems = await control("wrong-message", '//@ run-fail: attempt to divide by zero\nfn main() { panic!("another message") }\n');
  expect(problems).toEqual([expect.stringContaining("native Rust ended")]);
  expect(await control("unexpected-panic", "fn main() { let v: Vec<i32> = vec![]; v[0]; }\n")).toEqual([expect.stringContaining("native Rust ended")]);
}, 120_000);

test("an ignored case that passes is reported, so the list only shrinks", async () => {
  const problems = await control("passes", '//@ ignore-rust-js: a reason\nfn main() { println!("fine"); }\n');
  expect(problems).toEqual(["it passes now: remove `ignore-rust-js: a reason`"]);
}, 120_000);

test("a compile-fail case rust-js compiles, or rejects for another reason, is reported", async () => {
  expect(await control("compiles", "//@ compile-fail: does not support\nfn main() {}\n")).toEqual([
    "rust-js compiled it, but `compile-fail` says it can't",
  ]);
  expect(await control("other-error", "//@ compile-fail: a text no error has\nfn main() { let x: u128 = 1; println!(\"{x}\"); }\n")).toEqual([
    expect.stringContaining("rust-js's first error doesn't say"),
  ]);
}, 120_000);

test("unknown and repeated directives are reported", () => {
  expect(directives("//@ run-passes\nfn main() {}")).toContain("unknown or malformed directive");
  expect(directives("//@ run-fail\nfn main() {}")).toContain("unknown or malformed directive");
  expect(directives("//@ run_pass\nfn main() {}")).toContain("unknown or malformed directive");
  expect(directives("//@run-pass\nfn main() {}")).toContain("unknown or malformed directive");
  expect(directives("//@ run-pass\n//@ run-fail: x\nfn main() {}")).toBe("more than one directive");
  expect(directives("fn main() {}")).toEqual({ kind: "run-pass" });
});

// Where JS has no `process`, as in a browser, `print!` writes each line to
// `console.log` as it ends, and what's left when the task ends (ADR 0087). A
// whole line, `println!`'s, is written at once, before what's left.
test("print! without a process writes whole lines, and loses none", () => {
  const dir = fixture("corpus-print");
  const file = join(dir, "print.rs");
  writeFileSync(file, 'fn main() { print!("a"); print!("b\\nc"); print!("d"); eprint!("x\\n"); print!("\\n"); print!("left"); }\n');
  const compiled = compileJs(file, dir);
  if (!("js" in compiled)) throw new Error(compiled.error);
  const lines: string[] = [];
  const tasks: (() => void)[] = [];
  const context = {
    console: { log: (s: string) => lines.push(`log ${s}`), error: (s: string) => lines.push(`error ${s}`) },
    queueMicrotask: (task: () => void) => tasks.push(task),
  };
  const code = readFileSync(compiled.js, "utf8").replace(/^export /gm, "").replace(/^\/\/# sourceMappingURL=.*$/m, "");
  runInNewContext(`${code}\nentry();`, context);
  expect(lines).toEqual(["log ab", "error x", "log "]);
  // The task ends: what no line end wrote is written.
  for (const task of tasks) task();
  expect(lines).toEqual(["log ab", "error x", "log ", "log cdleft"]);
});

// A crate root may enable the features rust-js enables for itself, and
// choose its edition: rustc takes each only once.
test("a crate's own features and edition are its own", () => {
  const dir = fixture("corpus-root");
  const file = join(dir, "root.rs");
  writeFileSync(
    file,
    "#![feature(decl_macro, stmt_expr_attributes)]\n" +
      "macro double($x:expr) { $x * 2 }\n" +
      // Edition 2015's trait objects need no `dyn`.
      "pub fn f() -> i32 { let g: Box<Fn() -> i32> = Box::new(|| #[allow(unused_parens)] (3)); double!(g()) }\n",
  );
  const js = join(dir, "root.js");
  const p = Bun.spawnSync([compiler, file, "-o", js, "--", "--edition=2015", "-Awarnings"], { cwd: root, stderr: "pipe" });
  expect(p.stderr.toString()).toBe("");
  expect(p.exitCode).toBe(0);
  const run = Bun.spawnSync([process.execPath, "-e", `import(${JSON.stringify(js)}).then((m) => console.log(m.f()))`]);
  expect(run.stdout.toString()).toBe("6\n");
});

// What rustc works out for a program, rust-js's runtime agrees with: a
// `usize` is 32 bits in constants, `size_of` and `cfg`, as in its arithmetic,
// and `cfg(rust_js)` says it's rust-js (ADR 0090). Native Rust, on a 64-bit
// machine, can't be the oracle here.
test("rustc checks programs for rust-js's 32-bit usize", () => {
  const dir = fixture("corpus-target");
  const file = join(dir, "width.rs");
  writeFileSync(
    file,
    "const MAX: usize = usize::MAX;\n" +
      "const SIZE: usize = std::mem::size_of::<usize>();\n" +
      "pub fn width() -> String {\n" +
      "    let max = usize::MAX;\n" +
      "    let wrapped = max.wrapping_add(1);\n" +
      "    let who = if cfg!(rust_js) { \"rust-js\" } else { \"native\" };\n" +
      '    format!("{MAX} {max} {wrapped} {} {} {} {who}", SIZE, cfg!(target_pointer_width = "32"), MAX == max)\n' +
      "}\n",
  );
  const js = join(dir, "width.js");
  const p = Bun.spawnSync([compiler, file, "-o", js], { cwd: root, stderr: "pipe" });
  expect(p.stderr.toString()).toBe("");
  const run = Bun.spawnSync([process.execPath, "-e", `import(${JSON.stringify(js)}).then((m) => console.log(m.width()))`]);
  expect(run.stdout.toString()).toBe("4294967295 4294967295 0 4 true true rust-js\n");
});

// Which features a crate enables itself is read as Rust reads it: spaces
// anywhere, comments and strings aren't attributes. Found in review.
test("a crate's own features are read from its syntax, not its text", () => {
  const compiles = (name: string, source: string) => {
    const dir = fixture(`corpus-features-${name}`);
    const file = join(dir, "root.rs");
    writeFileSync(file, source + "macro double($x:expr) { $x * 2 }\npub fn f() -> i32 { double!(3) }\n");
    const p = Bun.spawnSync([compiler, file, "-o", join(dir, "root.js"), "--", "-Awarnings"], { cwd: root, stderr: "pipe" });
    return [name, p.exitCode === 0 ? "compiles" : p.stderr.toString().split("\n")[0]];
  };
  expect(compiles("spaced", "#![feature (decl_macro)]\n")).toEqual(["spaced", "compiles"]);
  expect(compiles("lines", "#![feature(\n    decl_macro,\n    stmt_expr_attributes,\n)]\n")).toEqual(["lines", "compiles"]);
  expect(compiles("documented", "//! A crate.\n/* #![feature(nothing)] */\n#![allow(unused)]\n#![feature(decl_macro)]\n")).toEqual(["documented", "compiles"]);
  // Only text: rust-js still enables what it needs.
  expect(compiles("commented", "// Enable it with #![feature(decl_macro)] if you need it.\n")).toEqual(["commented", "compiles"]);
  expect(compiles("quoted", 'pub const S: &str = "#![feature(decl_macro)]";\n')).toEqual(["quoted", "compiles"]);
});

// What a crate root enables through a `cfg_attr` is enabled only when its
// `cfg` holds, as rustc configures it, and a crate may register `rust_js`
// itself. An unfinished attribute is rustc's syntax error. Found in review.
test("a crate's own features and tools are as rustc configures them", () => {
  const compile = (name: string, source: string) => {
    const dir = fixture(`corpus-configured-${name}`);
    const file = join(dir, "root.rs");
    writeFileSync(file, source);
    const p = Bun.spawnSync([compiler, file, "-o", join(dir, "root.js"), "--", "-Awarnings"], { cwd: root, stderr: "pipe" });
    return [name, p.exitCode === 0 ? "compiles" : p.stderr.toString().split("\n")[0]];
  };
  const body = "macro double($x:expr) { $x * 2 }\npub fn f() -> i32 { #[rust_js::link_name = \"g\"] fn g() {} double!(3) }\n";
  expect(compile("enabled", "#![cfg_attr(all(), feature(decl_macro))]\n" + body)).toEqual(["enabled", "compiles"]);
  expect(compile("disabled", "#![cfg_attr(any(), feature(decl_macro))]\n" + body)).toEqual(["disabled", "compiles"]);
  expect(compile("several", "#![cfg_attr(all(), feature(register_tool, stmt_expr_attributes))]\n" + body)).toEqual(["several", "compiles"]);
  expect(compile("tool", "#![feature(register_tool)]\n#![register_tool(rust_js)]\n" + body)).toEqual(["tool", "compiles"]);
  expect(compile("tool-configured", "#![feature(register_tool)]\n#![cfg_attr(all(), register_tool(rust_js))]\n" + body)).toEqual(["tool-configured", "compiles"]);
  expect(compile("unfinished", "#![")).toEqual(["unfinished", "error: this file contains an unclosed delimiter"]);
});

// A `cfg` rustc doesn't expect, `FALSE` for `false`, is rustc's warning to
// give, once. rust-js's own early look at a crate root's `cfg`s raised it for
// no node, and rustc panicked. Found by rustc's `cfg-macros-notfoo`.
test("an unexpected cfg is rustc's warning, not a panic", () => {
  const dir = fixture("corpus-unexpected-cfg");
  const file = join(dir, "root.rs");
  writeFileSync(file, '#[cfg(FALSE)]\nmod shape {}\n#[cfg(not(FALSE))]\nmod shape {\n    pub fn name() -> &\'static str {\n        "shown"\n    }\n}\npub fn f() -> &\'static str {\n    shape::name()\n}\n');
  const js = join(dir, "root.js");
  const p = Bun.spawnSync([compiler, file, "-o", js], { cwd: root, stderr: "pipe" });
  const stderr = p.stderr.toString();
  expect(stderr).not.toContain("panicked");
  // As many as rustc gives, checking the same cfgs: one for each `FALSE`.
  const warnings = (text: string) => text.match(/warning: unexpected `cfg` condition name: `FALSE`/g)?.length;
  const native = Bun.spawnSync(["rustc", "--crate-type=lib", "--emit=metadata", "--check-cfg=cfg(browser, test, rust_js)", file, "-o", join(dir, "root.rmeta")], { cwd: root, stderr: "pipe" });
  expect(warnings(native.stderr.toString())).toBe(2);
  expect(warnings(stderr)).toBe(2);
  expect(p.exitCode).toBe(0);
  const run = Bun.spawnSync([process.execPath, "-e", `import(${JSON.stringify(js)}).then((m) => console.log(m.f()))`]);
  expect(run.stdout.toString()).toBe("shown\n");
});
