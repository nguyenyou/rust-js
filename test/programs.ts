// Running a Rust program natively and as JS, the same way, for the corpus
// (ADR 0088) and generated programs (ADR 0092): what it prints to stdout and
// stderr, and how its `main` ends, as the oracle says it (ADR 0088).

import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { compileFailure, runSync, stopped, type Exit } from "./child";
import { decode, encode, same, type Outcome } from "./oracle";
import { compiler, contentDirectory, nativeBinary, root, run } from "./support";

export const node = Bun.which("node");
// The generated JS runs under Node, the runtime rust-js targets (ADR 0095);
// Bun runs the tests, not the JS they check.
export const runtimes: [string, string[]][] = [["node", [node ?? "node"]]];

export type Call = { module: string; fn: string; args: unknown[] };

/** Each call, of the generated JS in `modules` (a name for each file), made
 * under Node by `node-calls.ts`, and what it did. */
export function callInNode(modules: Record<string, string>, calls: Call[], dir: string): Outcome[] {
  const callsFile = join(dir, "calls.json");
  const outcomesFile = join(dir, "outcomes.json");
  rmSync(outcomesFile, { force: true });
  writeFileSync(callsFile, encode({ modules, calls }));
  run([node ?? "node", join(root, "test/node-calls.ts"), callsFile, outcomesFile]);
  return decode(readFileSync(outcomesFile, "utf8"));
}

/** What a run printed, as text for its reports and as bytes to compare
 * (`Exit`), and how it ended. */
export type Run = { stdout: string; stderr: string; bytes: Exit["bytes"]; outcome: Outcome | string };

// A case runs for at most this long, so one that never ends fails instead
// of stopping the suite, and is compiled for at most the other, natively or
// by rust-js: `RUST_JS_COMPILE_TIMEOUT` shortens it for a test of a
// compiler that never ends.
const timeout = 10_000;
// A native binary's first run waits for macOS to check it, behind the
// other new ones when the test files run side by side (`nativeBinary`).
const firstRunTimeout = 60_000;
const compileTimeout = Number(process.env.RUST_JS_COMPILE_TIMEOUT ?? 120_000);

/** Runs `cmd`, which writes how `main` ended to `outcomeFile`, and says
 * what it printed and how it ended. A run that fails after writing its
 * outcome, as an unhandled rejection after `main` returns makes it, failed:
 * a run counts only if it exits 0, and only by the outcome it wrote itself. */
export function execute(cmd: string[], outcomeFile: string, limit = timeout): Run {
  rmSync(outcomeFile, { force: true });
  const p = runSync(cmd, root, limit);
  const { stdout, stderr, bytes } = p;
  const why = stopped(p, limit);
  if (why) return { stdout, stderr, bytes, outcome: why };
  if (!existsSync(outcomeFile)) return { stdout, stderr, bytes, outcome: `exited ${p.code} without an outcome` };
  const outcome = readFileSync(outcomeFile, "utf8");
  if (p.code !== 0) return { stdout, stderr, bytes, outcome: `exited ${p.code} after it ended ${outcome}` };
  return { stdout, stderr, bytes, outcome: JSON.parse(outcome) };
}

/** A string as a Rust string literal, for the wrappers' `include!`. */
const rustString = (s: string) => JSON.stringify(s);

// Natively, the case is a module whose `main` the wrapper calls, catching a
// panic as the JS runner does. The panic hook is silenced, so stderr is only
// what the program writes. The case is included where it is, so what it
// reads beside it, an `include_str!`, is found as the JS's compile finds it;
// the wrapper is where its text says, so a case is one kept binary.
export function runNative(file: string, dir: string, edition = "2024"): Run | string {
  const source = `mod case {
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
`;
  const built = nativeBinary(source, contentDirectory(source), [`--edition=${edition}`, "-Coverflow-checks=off", "-Awarnings"], compileTimeout);
  if ("error" in built) return built.error;
  const outcomeFile = join(dir, "native.json");
  return execute([built.binary, outcomeFile], outcomeFile, built.built ? firstRunTimeout : timeout);
}

/** A compile that failed: rust-js's clear rejection, or a crash, however
 * it began (`compileFailure`), with everything it said. */
export type CompileError = { kind: "rejected" | "crashed"; reason: string; error: string };

// As JS, the case is the crate's root, with `entry` exported to call `main`.
export function compileJs(file: string, dir: string, edition = "2024"): { js: string } | CompileError {
  const wrapper = join(dir, "lib.rs");
  writeFileSync(wrapper, `include!(${rustString(file)});\npub fn entry() {\n    main()\n}\n`);
  const js = join(dir, "case.js");
  const p = runSync([compiler, wrapper, "-o", js, "--", `--edition=${edition}`, "-Awarnings"], root, compileTimeout);
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
  a.bytes.stdout.equals(b.bytes.stdout) &&
  a.bytes.stderr.equals(b.bytes.stderr) &&
  typeof a.outcome !== "string" &&
  typeof b.outcome !== "string" &&
  same(a.outcome, b.outcome);
