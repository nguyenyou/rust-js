// Running a Rust program natively and as JS, the same way, for the corpus
// (ADR 0088) and generated programs (ADR 0092): what it prints to stdout and
// stderr, and how its `main` ends, as the oracle says it (ADR 0088).

import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { compileFailure, runSync, stopped } from "./child";
import { same, type Outcome } from "./oracle";
import { compiler, root } from "./support";

export const node = Bun.which("node");
export const runtimes: [string, string[]][] = [
  ["bun", [process.execPath]],
  ["node", [node ?? "node"]],
];

export type Run = { stdout: string; stderr: string; outcome: Outcome | string };

// A case runs for at most this long, so one that never ends fails instead
// of stopping the suite, and is compiled for at most the other, natively or
// by rust-js: `RUST_JS_COMPILE_TIMEOUT` shortens it for a test of a
// compiler that never ends.
const timeout = 10_000;
const compileTimeout = Number(process.env.RUST_JS_COMPILE_TIMEOUT ?? 120_000);

/** Runs `cmd`, which writes how `main` ended to `outcomeFile`, and says
 * what it printed and how it ended. A run that fails after writing its
 * outcome, as an unhandled rejection after `main` returns makes it, failed:
 * a run counts only if it exits 0, and only by the outcome it wrote itself. */
export function execute(cmd: string[], outcomeFile: string): Run {
  rmSync(outcomeFile, { force: true });
  const p = runSync(cmd, root, timeout);
  const { stdout, stderr } = p;
  const why = stopped(p, timeout);
  if (why) return { stdout, stderr, outcome: why };
  if (!existsSync(outcomeFile)) return { stdout, stderr, outcome: `exited ${p.code} without an outcome` };
  const outcome = readFileSync(outcomeFile, "utf8");
  if (p.code !== 0) return { stdout, stderr, outcome: `exited ${p.code} after it ended ${outcome}` };
  return { stdout, stderr, outcome: JSON.parse(outcome) };
}

/** A string as a Rust string literal, for the wrappers' `include!`. */
const rustString = (s: string) => JSON.stringify(s);

// Natively, the case is a module whose `main` the wrapper calls, catching a
// panic as the JS runner does. The panic hook is silenced, so stderr is only
// what the program writes.
export function runNative(file: string, dir: string): Run | string {
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
  const build = runSync(["rustc", "--edition=2024", "-Coverflow-checks=off", "-Awarnings", wrapper, "-o", binary], root, compileTimeout);
  const why = stopped(build, compileTimeout);
  if (why || build.code !== 0) return `rustc can't compile it${why ? `: it ${why}` : ""}:\n${build.stderr}`;
  const outcomeFile = join(dir, "native.json");
  return execute([binary, outcomeFile], outcomeFile);
}

/** A compile that failed: rust-js's clear rejection, or a crash, however
 * it began (`compileFailure`), with everything it said. */
export type CompileError = { kind: "rejected" | "crashed"; reason: string; error: string };

// As JS, the case is the crate's root, with `entry` exported to call `main`.
export function compileJs(file: string, dir: string): { js: string } | CompileError {
  const wrapper = join(dir, "lib.rs");
  writeFileSync(wrapper, `include!(${rustString(file)});\npub fn entry() {\n    main()\n}\n`);
  const js = join(dir, "case.js");
  const p = runSync([compiler, wrapper, "-o", js, "--", "-Awarnings"], root, compileTimeout);
  if (p.code === 0 && !stopped(p, compileTimeout)) return { js };
  const { kind, reason } = compileFailure(p, compileTimeout);
  return { kind, reason, error: kind === "crashed" ? `rust-js crashed: ${reason}\n${p.stderr}` : p.stderr };
}

export function runJs(runtime: string[], js: string, dir: string, name: string): Run {
  const outcomeFile = join(dir, `${name}.json`);
  return execute([...runtime, join(root, "test/corpus-run.ts"), js, outcomeFile], outcomeFile);
}

export const show = (outcome: Outcome | string) => (typeof outcome === "string" ? outcome : JSON.stringify(outcome));
export const agree = (a: Run, b: Run) =>
  a.stdout === b.stdout &&
  a.stderr === b.stderr &&
  typeof a.outcome !== "string" &&
  typeof b.outcome !== "string" &&
  same(a.outcome, b.outcome);
