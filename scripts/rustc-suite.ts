// Runs rustc's own `run-pass` UI tests, at the pinned toolchain's commit, as
// corpus cases (ADR 0089): each is built natively and with rust-js, and the
// JS must print what the native binary prints, under Bun and Node.
//
//   bun scripts/rustc-suite.ts            # check against the known failures
//   bun scripts/rustc-suite.ts --bless    # rewrite the known failures
//   bun scripts/rustc-suite.ts derives/ path/to/test.rs    # run some, and say how each did
//   bun scripts/rustc-suite.ts --shard=2/4 --out=r2.json   # every fourth test, from the second
//   bun scripts/rustc-suite.ts --merge r1.json r2.json ..  # the shards, checked as one run
//   bun scripts/rustc-suite.ts --compiler=target/debug/rust-js ..
//
// A test rust-js gets wrong is listed, with its first error, in
// test/rustc-known-failures.txt. One that isn't listed must pass, and one
// that is must still fail: when it passes, it's taken off, so the list only
// shrinks.

import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { availableParallelism, homedir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";

import { compileFailure, run, stopped, type Exit } from "../test/child";
import { rustcTests } from "./rustc-tests";

const root = join(import.meta.dir, "..");
// The rust-js that compiles each test: a release build, as the known
// failures are made with, since a debug build's deeper stack overflows on
// tests a release build passes. `--compiler=path` says another.
let compiler = join(root, "target", "release", "rust-js");
const knownFile = join(root, "test", "rustc-known-failures.txt");
const work = join(root, "target", "rustc-suite");

// A directive whose test needs what a single program run as JS can't have,
// and why. Every other `run-pass` test is in scope.
const outOfScope: [RegExp, string][] = [
  [/^(aux-build|aux-crate|aux-bin|proc-macro)$/, "needs another crate"],
  [/^revisions$/, "has revisions"],
  [/^(compile-flags|rustc-env|exec-env|run-flags|unset-exec-env)$/, "needs flags or an environment of its own"],
  [/^needs-(?!unwind$)/, "needs a capability of its own"],
  [/^only-/, "is for some targets only"],
  [/^ignore-(wasm|wasm32|wasm32-bare)$/, "doesn't apply to wasm, whose integers rust-js has"],
  [/^known-bug$/, "is a known rustc bug"],
  [/^ignore-test$/, "is disabled"],
];

export type Scope = { edition: string } | { skip: string };

/** Whether a test fits a corpus case, and its edition: 2015, unless it says. */
export function scope(source: string): Scope {
  let edition = "2015";
  // Every `//@` line: its name is what starts it, whatever follows.
  for (const [, line] of source.matchAll(/^\/\/@\s*(.*)$/gm)) {
    const [, name = "", value] = /^([A-Za-z0-9_.-]*)\s*(?::\s*(.*))?/.exec(line) ?? [];
    const out = outOfScope.find(([pattern]) => pattern.test(name));
    if (out) return { skip: out[1] };
    if (name === "edition" && value) edition = value.trim();
  }
  if (!/^\s*(pub\s+)?fn main\s*\(\s*\)\s*\{/m.test(source)) return { skip: "has no `fn main() {`" };
  if (/^\s*(pub\s+)?mod\s+\w+\s*;/m.test(source)) return { skip: "has modules in other files" };
  if (/\binclude(_str|_bytes)?!\s*\(/.test(source)) return { skip: "reads files beside it" };
  if (/\bfeature\([^)]*\bstaged_api\b/.test(source)) return { skip: "is the standard library's own API" };
  return { edition };
}

export type Result = { test: string; status: "pass" } | { test: string; status: "fail" | "skip"; reason: string };

/** How a test fails, from what its reason says: `rejected`, rust-js's
 * own clear error; `crashed`, another compile error, such as a panic of
 * rustc's; or `wrong`, JS that ran otherwise than native Rust. */
export function failureKind(reason: string): "rejected" | "crashed" | "wrong" {
  if (/^(bun|node): /.test(reason)) return "wrong";
  return /^error: rust-js( does not support|:)/.test(reason) ? "rejected" : "crashed";
}

/** What's changed since the known failures were written: a test that fails
 * and isn't listed, one that's listed and passes, and one that fails worse
 * than it's listed as, a clear rejection now a crash or a wrong answer. */
export function ratchet(results: Result[], known: Map<string, string>) {
  const failing = results.filter((r): r is Result & { reason: string } => r.status === "fail");
  return {
    regressions: failing.filter((r) => !known.has(r.test)),
    fixed: results.filter((r) => r.status === "pass" && known.has(r.test)),
    worse: failing.filter((r) => {
      const listed = known.get(r.test);
      return listed !== undefined && failureKind(listed) === "rejected" && failureKind(r.reason) !== "rejected";
    }),
  };
}

/** The tests of a run of some that aren't as the known failures say, as
 * the ratchet says of a whole run. */
export function surprises(results: Result[], known: Map<string, string>): Set<string> {
  const { regressions, fixed, worse } = ratchet(results, known);
  return new Set([...regressions, ...fixed, ...worse].map((r) => r.test));
}

/** A diagnostic's first line, as short as it can say it, with nothing of
 * this machine's or this run's: its paths, its toolchain's host, a
 * thread's number. The list is the same wherever it's written. */
export const firstError = (stderr: string) =>
  normalize(stderr.split("\n").find((line) => line.startsWith("error")) ?? stderr.trim().split("\n")[0] ?? "no output");
const normalize = (line: string) =>
  line
    .replaceAll(/\/[^\s`:]*\/rustc-suite\/case-[^/\s`:]*/g, "<case>")
    .replaceAll(homedir(), "~")
    .replaceAll(/\.rustup\/toolchains\/[^/\s]+/g, ".rustup/toolchains/<toolchain>")
    .replaceAll(/thread '([^']*)' \(\d+\)/g, "thread '$1'")
    .slice(0, 200);

/** Did a process end otherwise than by exiting 0? */
const failed = (exit: Exit, timeout: number) => exit.code !== 0 || stopped(exit, timeout) !== undefined;

async function runTest(ui: string, file: string): Promise<Result> {
  const test = relative(ui, file);
  const source = readFileSync(file, "utf8");
  const s = scope(source);
  if ("skip" in s) return { test, status: "skip", reason: s.skip };
  const dir = mkdtempSync(join(work, "case-"));
  try {
    // Natively, as rust-js takes Rust: the release profile, where arithmetic wraps.
    const binary = join(dir, "native");
    const build = await run(
      ["rustc", `--edition=${s.edition}`, "-Coverflow-checks=off", "-Awarnings", file, "-o", binary],
      dirname(file),
      120_000,
    );
    if (failed(build, 120_000)) return { test, status: "skip", reason: `rustc: ${stopped(build, 120_000) ?? firstError(build.stderr)}` };
    const native = await run([binary], dirname(file), 10_000);
    if (failed(native, 10_000)) return { test, status: "skip", reason: "doesn't pass natively with overflow checks off" };
    // What a `HashMap` prints, say, changes from run to run: no answer to compare with.
    for (let i = 0; i < 2; i++) {
      const again = await run([binary], dirname(file), 10_000);
      if (failed(again, 10_000)) return { test, status: "skip", reason: "doesn't pass natively on every run" };
      if (again.stdout !== native.stdout || again.stderr !== native.stderr) {
        return { test, status: "skip", reason: "prints what changes from run to run" };
      }
    }
    rmSync(binary, { force: true });

    // As JS: the test, with `main` exported to call.
    // Named as the test is, so its crate is: `module_path!()` says it.
    const lib = join(dir, basename(file));
    writeFileSync(lib, `${source}\n/// The test's main, for the JS to call.\npub fn entry() {\n    main()\n}\n`);
    const js = join(dir, "case.js");
    const compiled = await run([compiler, lib, "-o", js, "--", `--edition=${s.edition}`, "-Awarnings"], dir, 120_000);
    if (failed(compiled, 120_000)) {
      // Its first error, if it's rust-js's rejection; what crashed, if not,
      // even after a rejection.
      const failure = compileFailure(compiled, 120_000);
      return { test, status: "fail", reason: failure.kind === "rejected" ? firstError(compiled.stderr) : normalize(failure.reason) };
    }
    for (const [name, runtime] of [["bun", process.execPath], ["node", "node"]]) {
      const outcomeFile = join(dir, `${name}.json`);
      const ran = await run([runtime, join(root, "test", "corpus-run.ts"), js, outcomeFile], dir, 10_000);
      const why = stopped(ran, 10_000);
      if (why) return { test, status: "fail", reason: `${name}: ${why}` };
      // It counts only if it exits 0: one that fails after writing how `main`
      // ended, as an unhandled rejection makes it, failed.
      const written = existsSync(outcomeFile) ? readFileSync(outcomeFile, "utf8") : undefined;
      const outcome = written !== undefined && ran.code === 0 ? written : `exited ${ran.code}: ${written ?? firstError(ran.stderr)}`;
      if (outcome !== '{"value":null}') return { test, status: "fail", reason: `${name}: ended ${outcome.slice(0, 200)}` };
      if (ran.stdout !== native.stdout) return { test, status: "fail", reason: `${name}: different stdout` };
      if (ran.stderr !== native.stderr) return { test, status: "fail", reason: `${name}: different stderr` };
    }
    return { test, status: "pass" };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function findTests(ui: string): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "auxiliary") walk(path);
      } else if (entry.name.endsWith(".rs") && /^\/\/@\s*run-pass\s*$/m.test(readFileSync(path, "utf8"))) {
        found.push(path);
      }
    }
  };
  walk(ui);
  return found.sort();
}

/** `test<TAB>reason` lines: what rust-js gets wrong, and how. */
function readKnown(): Map<string, string> {
  const known = new Map<string, string>();
  if (!existsSync(knownFile)) return known;
  for (const line of readFileSync(knownFile, "utf8").split("\n")) {
    if (line === "" || line.startsWith("#")) continue;
    const [test, reason = ""] = line.split("\t");
    known.set(test, reason);
  }
  return known;
}

/** Every `run-pass` test under `dir`, or `dir` itself if it's one. */
function testsUnder(ui: string, selector: string): string[] {
  const path = join(ui, selector);
  if (!existsSync(path)) throw new Error(`no test or directory ${selector} in tests/ui`);
  return statSync(path).isDirectory() ? findTests(path) : [path];
}

async function runAll(ui: string, tests: string[]): Promise<Result[]> {
  mkdirSync(work, { recursive: true });
  const results: Result[] = [];
  let next = 0;
  const worker = async () => {
    while (next < tests.length) {
      const file = tests[next++];
      results.push(await runTest(ui, file));
      if (results.length % 250 === 0) console.error(`${results.length}/${tests.length}`);
    }
  };
  await Promise.all(Array.from({ length: availableParallelism() }, worker));
  return results.sort((a, b) => a.test.localeCompare(b.test));
}

function summarize(results: Result[]) {
  const count = (status: string) => results.filter((r) => r.status === status).length;
  const skipped = new Map<string, number>();
  for (const r of results) if ("reason" in r && r.status === "skip") skipped.set(r.reason, (skipped.get(r.reason) ?? 0) + 1);
  return {
    tests: results.length,
    inScope: count("pass") + count("fail"),
    pass: count("pass"),
    fail: count("fail"),
    skipped: Object.fromEntries([...skipped].sort((a, b) => b[1] - a[1]).filter(([reason]) => !reason.startsWith("rustc: "))),
    skippedByRustc: [...skipped].filter(([reason]) => reason.startsWith("rustc: ")).reduce((n, [, k]) => n + k, 0),
  };
}

/** Markdown for a GitHub run's page, when there is one. */
function toSummary(lines: string[]) {
  const file = process.env.GITHUB_STEP_SUMMARY;
  if (file) writeFileSync(file, lines.join("\n") + "\n", { flag: "a" });
}

const cell = (s: string) => s.replaceAll("|", "\\|").replaceAll("\n", " ");

/** A whole run's results, against the known failures: rewritten with
 * `bless`, else checked, and the process fails if they've changed. */
function report(results: Result[], bless: boolean) {
  const summary = summarize(results);
  console.log(JSON.stringify(summary, null, 2));
  toSummary([
    "## rustc run-pass tests",
    "",
    `**${summary.pass}** of ${summary.inScope} in scope pass; ${summary.tests - summary.inScope} out of scope.`,
    "",
  ]);
  const failing = results.filter((r): r is Result & { reason: string } => r.status === "fail");
  if (bless) {
    const header = "# rustc run-pass UI tests rust-js gets wrong, and its first error (ADR 0089).\n# Rewritten by `bun scripts/rustc-suite.ts --bless`.\n";
    writeFileSync(knownFile, header + failing.map((r) => `${r.test}\t${r.reason}\n`).join(""));
    console.log(`wrote ${failing.length} known failures`);
    toSummary([`Wrote ${failing.length} known failures.`]);
    return;
  }
  const known = readKnown();
  const { regressions, fixed, worse } = ratchet(results, known);
  for (const r of regressions) console.log(`FAILS\t${r.test}\t${r.reason}`);
  for (const r of fixed) console.log(`PASSES\t${r.test}\tremove it from test/rustc-known-failures.txt`);
  for (const r of worse) console.log(`WORSE\t${r.test}\t${failureKind(r.reason)}, listed as rejected: ${r.reason}`);
  if (regressions.length === 0 && fixed.length === 0 && worse.length === 0) {
    toSummary(["The known failures are as listed."]);
    return;
  }
  toSummary([
    "| Test | Now | |",
    "|---|---|---|",
    ...regressions.map((r) => `| ${r.test} | fails | ${cell(r.reason)} |`),
    ...fixed.map((r) => `| ${r.test} | passes | take it off the known failures |`),
    ...worse.map((r) => `| ${r.test} | ${failureKind(r.reason)} | listed as rejected: ${cell(r.reason)} |`),
  ]);
  console.log(`${regressions.length} newly failing, ${fixed.length} newly passing, ${worse.length} failing worse: run with --bless once they're intended`);
  process.exitCode = 1;
}

async function main() {
  const args = process.argv.slice(2);
  const bless = args.includes("--bless");
  const option = (name: string) => args.find((a) => a.startsWith(`--${name}=`))?.slice(name.length + 3);
  const out = option("out");
  const shard = option("shard");
  // `--merge a.json b.json`: the shards' results, as one run.
  if (args.includes("--merge")) {
    const files = args.filter((a) => !a.startsWith("--"));
    const results = files.flatMap((f) => JSON.parse(readFileSync(f, "utf8")) as Result[]);
    report(results.sort((a, b) => a.test.localeCompare(b.test)), bless);
    return;
  }
  const selectors = args.filter((a) => !a.startsWith("--"));
  compiler = resolve(option("compiler") ?? compiler);
  const ui = rustcTests();
  if (!existsSync(compiler)) throw new Error(`no rust-js at ${compiler}: build it with cargo build --release`);
  let tests = selectors.length > 0 ? [...new Set(selectors.flatMap((s) => testsUnder(ui, s)))] : findTests(ui);
  // `--shard=2/4`: every fourth test, from the second.
  if (shard) {
    const [i, n] = shard.split("/").map(Number);
    if (!(n > 0 && i >= 1 && i <= n)) throw new Error(`--shard=${shard}: say which of how many, as 2/4`);
    tests = tests.filter((_, k) => k % n === i - 1);
  }
  const results = await runAll(ui, tests);
  if (out) writeFileSync(out, JSON.stringify(results));
  if (shard) {
    console.log(JSON.stringify(summarize(results), null, 2));
    return;
  }
  if (selectors.length > 0) {
    // Some tests: each, and whether it's what the known failures say.
    const unlisted = surprises(results, readKnown());
    const rows = results.map((r) => ({ r, surprise: unlisted.has(r.test) }));
    for (const { r, surprise } of rows) {
      console.log(`${r.status}\t${r.test}${"reason" in r ? `\t${r.reason}` : ""}${surprise ? "\t(not as the known failures say)" : ""}`);
    }
    console.log(JSON.stringify(summarize(results), null, 2));
    toSummary([
      `## ${results.length} rustc tests`,
      "",
      "| Test | Result | Reason |",
      "|---|---|---|",
      ...rows.map(({ r, surprise }) => `| ${r.test} | ${r.status}${surprise ? " ⚠️ not as listed" : ""} | ${"reason" in r ? cell(r.reason) : ""} |`),
    ]);
    if (rows.some((row) => row.surprise)) process.exitCode = 1;
    return;
  }
  // The whole run's results, which a run of some tests leaves as they were.
  writeFileSync(join(work, "results.json"), JSON.stringify(results, null, 2));
  report(results, bless);
}

if (import.meta.main) await main();
