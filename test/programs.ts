// Running a Rust program natively and as JS, the same way, for the corpus
// (ADR 0088) and generated programs (ADR 0092): what it prints to stdout and
// stderr, and how its `main` ends, as the oracle says it (ADR 0088).

import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { same, type Outcome } from "./oracle";
import { compiler, root } from "./support";

export const node = Bun.which("node");
export const runtimes: [string, string[]][] = [
  ["bun", [process.execPath]],
  ["node", [node ?? "node"]],
];

export type Run = { stdout: string; stderr: string; outcome: Outcome | string };

// A case runs for at most this long, so one that never ends fails instead
// of stopping the suite.
const timeout = 10_000;

/** Runs `cmd`, which writes how `main` ended to `outcomeFile`, and says
 * what it printed and how it ended. A run that fails after writing its
 * outcome, as an unhandled rejection after `main` returns makes it, failed:
 * a run counts only if it exits 0, and only by the outcome it wrote itself. */
export function execute(cmd: string[], outcomeFile: string): Run {
  rmSync(outcomeFile, { force: true });
  const p = Bun.spawnSync(cmd, { cwd: root, stdout: "pipe", stderr: "pipe", timeout });
  const stdout = p.stdout.toString();
  const stderr = p.stderr.toString();
  if (p.signalCode) return { stdout, stderr, outcome: `killed by ${p.signalCode} (a ${timeout / 1000}s limit)` };
  if (!existsSync(outcomeFile)) return { stdout, stderr, outcome: `exited ${p.exitCode} without an outcome` };
  const outcome = readFileSync(outcomeFile, "utf8");
  if (p.exitCode !== 0) return { stdout, stderr, outcome: `exited ${p.exitCode} after it ended ${outcome}` };
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
  const build = Bun.spawnSync(["rustc", "--edition=2024", "-Coverflow-checks=off", "-Awarnings", wrapper, "-o", binary], {
    cwd: root,
    stderr: "pipe",
  });
  if (build.exitCode !== 0) return `rustc can't compile it:\n${build.stderr.toString()}`;
  const outcomeFile = join(dir, "native.json");
  return execute([binary, outcomeFile], outcomeFile);
}

// As JS, the case is the crate's root, with `entry` exported to call `main`.
export function compileJs(file: string, dir: string): { js: string } | { error: string } {
  const wrapper = join(dir, "lib.rs");
  writeFileSync(wrapper, `include!(${rustString(file)});\npub fn entry() {\n    main()\n}\n`);
  const js = join(dir, "case.js");
  const p = Bun.spawnSync([compiler, wrapper, "-o", js, "--", "-Awarnings"], { cwd: root, stderr: "pipe" });
  return p.exitCode === 0 ? { js } : { error: p.stderr.toString() };
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
