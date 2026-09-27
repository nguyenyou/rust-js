// The corpus: Rust programs, each a `fn main()` as rustc's own tests are,
// run natively and as JS under Bun and Node, which must print the same and
// end the same (ADR 0088). What a case expects is in its `//@` directives:
//
//   //@ run-pass                   main returns (the default)
//   //@ run-fail: <message>        main panics with exactly this message (\n for a newline)
//   //@ compile-fail: <text>       rust-js rejects it, with this in its error
//   //@ ignore-rust-js: <reason>   rust-js gets it wrong for now; passing is an error
//
//   case.rs ─┬─ rustc ──► native ─────────────────┐
//            └─ rust-js ──► case.js ─┬─ bun  ──────┼─► stdout, stderr, outcome: the same?
//                                    └─ node ──────┘

import { beforeAll, expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { runInNewContext } from "node:vm";

import { expected, same, type Outcome } from "./oracle";
import { buildCompiler, compiler, fixture, root } from "./support";

const corpus = join(root, "test/corpus");
const node = Bun.which("node");
const runtimes: [string, string[]][] = [
  ["bun", [process.execPath]],
  ["node", [node ?? "node"]],
];

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

type Run = { stdout: string; stderr: string; outcome: Outcome | string };

// A case runs for at most this long, so one that never ends fails instead
// of stopping the suite.
const timeout = 10_000;

function execute(cmd: string[], outcomeFile: string): Run {
  const p = Bun.spawnSync(cmd, { cwd: root, stdout: "pipe", stderr: "pipe", timeout });
  const stdout = p.stdout.toString();
  const stderr = p.stderr.toString();
  if (p.signalCode) return { stdout, stderr, outcome: `killed by ${p.signalCode} (a ${timeout / 1000}s limit)` };
  if (!existsSync(outcomeFile)) return { stdout, stderr, outcome: `exited ${p.exitCode} without an outcome` };
  return { stdout, stderr, outcome: JSON.parse(readFileSync(outcomeFile, "utf8")) };
}

/** A string as a Rust string literal, for the wrappers' `include!`. */
const rustString = (s: string) => JSON.stringify(s);

// Natively, the case is a module whose `main` the wrapper calls, catching a
// panic as the JS runner does. The panic hook is silenced, so stderr is only
// what the program writes.
function runNative(file: string, dir: string): Run | string {
  const wrapper = join(dir, "native.rs");
  writeFileSync(wrapper, `mod case {
    include!(${rustString(file)});
    pub fn entry() { main() }
}
fn json(s: &str) -> String {
    let mut out = String::from("\\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\\\\""),
            '\\\\' => out.push_str("\\\\\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = match std::panic::catch_unwind(case::entry) {
        Ok(()) => String::from("{\\"value\\":null}"),
        Err(e) => match e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()) {
            Some(message) => format!("{{\\"panic\\":{}}}", json(&message)),
            None => String::from("\\"a panic whose payload isn't a string\\""),
        },
    };
    std::fs::write(std::env::args().nth(1).expect("an outcome file"), outcome).expect("the outcome is written");
}
`);
  const binary = join(dir, "native");
  const build = Bun.spawnSync(["rustc", "--edition=2024", "-Coverflow-checks=off", "-Awarnings", wrapper, "-o", binary], {
    cwd: root,
    stderr: "pipe",
  });
  if (build.exitCode !== 0) return `rustc can't compile it:\n${build.stderr.toString()}`;
  const outcomeFile = join(dir, "native.json");
  return execute([binary, outcomeFile], outcomeFile);
}

// As JS, the case is the crate's root, with `entry` exported to call `main`.
function compileJs(file: string, dir: string): { js: string } | { error: string } {
  const wrapper = join(dir, "lib.rs");
  writeFileSync(wrapper, `include!(${rustString(file)});\npub fn entry() {\n    main()\n}\n`);
  const js = join(dir, "case.js");
  const p = Bun.spawnSync([compiler, wrapper, "-o", js, "--", "-Awarnings"], { cwd: root, stderr: "pipe" });
  return p.exitCode === 0 ? { js } : { error: p.stderr.toString() };
}

function runJs(runtime: string[], js: string, dir: string, name: string): Run {
  const outcomeFile = join(dir, `${name}.json`);
  return execute([...runtime, join(root, "test/corpus-run.ts"), js, outcomeFile], outcomeFile);
}

const show = (outcome: Outcome | string) => (typeof outcome === "string" ? outcome : JSON.stringify(outcome));
const agree = (a: Run, b: Run) =>
  a.stdout === b.stdout &&
  a.stderr === b.stderr &&
  typeof a.outcome !== "string" &&
  typeof b.outcome !== "string" &&
  same(a.outcome, b.outcome);

/** What's wrong with a case: nothing, if native Rust does what its
 * directives say and the JS does what native Rust does. */
function check(file: string): string[] {
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
    return compiled.error.includes(want.text) ? [] : [`rust-js's error doesn't say \`${want.text}\`:\n${compiled.error}`];
  }
  const problems: string[] = [];
  const runs: [string, Run][] = "js" in compiled ? runtimes.map(([name, cmd]) => [name, runJs(cmd, compiled.js, dir, name)]) : [];
  if (want.kind === "ignore-rust-js") {
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
  test(name, () => {
    expect(check(join(corpus, name))).toEqual([]);
  }, 120_000);
}

// Negative controls: each of these cases is wrong, and must be reported.
function control(name: string, source: string): string[] {
  const file = join(fixture("corpus-control"), `${name}.rs`);
  writeFileSync(file, source);
  return check(file);
}

test("a directive native Rust disagrees with is reported", () => {
  const problems = control("wrong-message", '//@ run-fail: attempt to divide by zero\nfn main() { panic!("another message") }\n');
  expect(problems).toEqual([expect.stringContaining("native Rust ended")]);
  expect(control("unexpected-panic", "fn main() { let v: Vec<i32> = vec![]; v[0]; }\n")).toEqual([expect.stringContaining("native Rust ended")]);
}, 120_000);

test("an ignored case that passes is reported, so the list only shrinks", () => {
  const problems = control("passes", '//@ ignore-rust-js: a reason\nfn main() { println!("fine"); }\n');
  expect(problems).toEqual(["it passes now: remove `ignore-rust-js: a reason`"]);
}, 120_000);

test("a compile-fail case rust-js compiles, or rejects for another reason, is reported", () => {
  expect(control("compiles", "//@ compile-fail: does not support\nfn main() {}\n")).toEqual([
    "rust-js compiled it, but `compile-fail` says it can't",
  ]);
  expect(control("other-error", "//@ compile-fail: a text no error has\nfn main() { let x: u128 = 1; println!(\"{x}\"); }\n")).toEqual([
    expect.stringContaining("rust-js's error doesn't say"),
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
