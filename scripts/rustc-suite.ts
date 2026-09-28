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
// shrinks. A test native Rust gives no answer for, as it can't build or run
// it, is listed in test/rustc-native-failures.txt, and one out of scope in
// test/rustc-out-of-scope.txt, which change only by a bless too.

import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { availableParallelism, homedir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";

import { compileFailure, run, stopped, type Exit } from "../test/child";
import { rustcCommit, rustcTests } from "./rustc-tests";

const root = join(import.meta.dir, "..");
// The rust-js that compiles each test: a release build, as the known
// failures are made with, since a debug build's deeper stack overflows on
// tests a release build passes. `--compiler=path` says another.
let compiler = join(root, "target", "release", "rust-js");
const knownFile = join(root, "test", "rustc-known-failures.txt");
// Tests native Rust can't give an answer for here: it can't build or run
// them, or they print what changes from run to run. They can't be compared,
// so they're listed, and one that native Rust newly can't run, or now
// can, fails the run as a changed known failure does.
const nativeFile = join(root, "test", "rustc-native-failures.txt");
// Tests out of scope, as their source says, listed too: a scope rule that
// takes in a test, or a test that leaves, changes coverage, and fails the
// run until it's blessed.
const outOfScopeFile = join(root, "test", "rustc-out-of-scope.txt");
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
    // A range, `2015..2021` or `2021..`, is half-open, and compiletest
    // runs it by default at its lowest edition.
    if (name === "edition" && value) edition = value.split("..")[0].trim();
  }
  if (!/^\s*(pub\s+)?fn main\s*\(\s*\)\s*\{/m.test(source)) return { skip: "has no `fn main() {`" };
  if (/^\s*(pub\s+)?mod\s+\w+\s*;/m.test(source)) return { skip: "has modules in other files" };
  if (/\binclude(_str|_bytes)?!\s*\(/.test(source)) return { skip: "reads files beside it" };
  if (/\bfeature\([^)]*\bstaged_api\b/.test(source)) return { skip: "is the standard library's own API" };
  return { edition };
}

// `skip` is a test out of scope, by what its source says; `native`, one
// native Rust gives no answer for.
export type Result = { test: string; status: "pass" } | { test: string; status: "fail" | "skip" | "native"; reason: string };

/** How a test fails, from what its reason says: `rejected`, rust-js's
 * own clear error; `crashed`, another compile error, such as a panic of
 * rustc's; or `wrong`, JS that ran otherwise than native Rust. */
export function failureKind(reason: string): "rejected" | "crashed" | "wrong" {
  if (/^(bun|node): /.test(reason)) return "wrong";
  return /^error: rust-js( does not support|:)/.test(reason) ? "rejected" : "crashed";
}

/** What's changed since the known failures were written: a test that fails
 * and isn't listed, one that's listed and passes, and one that fails worse
 * than it's listed as, a clear rejection now a crash or a wrong answer; one
 * native Rust gives no answer for that isn't listed, or that's listed and
 * native Rust now answers; and one out of scope that isn't listed, or
 * that's listed and in scope now. */
export function ratchet(
  results: Result[],
  known: Map<string, string>,
  native = new Map<string, string>(),
  outOfScope = new Map<string, string>(),
) {
  const failing = results.filter((r): r is Result & { reason: string } => r.status === "fail");
  return {
    regressions: failing.filter((r) => !known.has(r.test)),
    fixed: results.filter((r) => r.status === "pass" && known.has(r.test)),
    worse: failing.filter((r) => {
      const listed = known.get(r.test);
      return listed !== undefined && failureKind(listed) === "rejected" && failureKind(r.reason) !== "rejected";
    }),
    unanswered: results.filter((r): r is Result & { reason: string } => r.status === "native" && !native.has(r.test)),
    answered: results.filter((r) => r.status !== "native" && native.has(r.test)),
    excluded: results.filter((r): r is Result & { reason: string } => r.status === "skip" && !outOfScope.has(r.test)),
    included: results.filter((r) => r.status !== "skip" && outOfScope.has(r.test)),
  };
}

/** The tests of a run of some that aren't as the known failures say, as
 * the ratchet says of a whole run. */
export function surprises(
  results: Result[],
  known: Map<string, string>,
  native = new Map<string, string>(),
  outOfScope = new Map<string, string>(),
): Set<string> {
  const changed = ratchet(results, known, native, outOfScope);
  return new Set(Object.values(changed).flatMap((list) => list.map((r: Result) => r.test)));
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

/** A shard's run, as `--shard --out` writes it: which of how many it
 * is, what it ran with, every test there was, the ones it was to run, and
 * how each did. */
export type Shard = {
  shard: number;
  of: number;
  compiler: string;
  toolchain: string;
  source: string;
  inventory: string[];
  expected: string[];
  results: Result[];
};

/** What's wrong with shards taken as one run, or nothing: each is there
 * once, all ran the same compiler, toolchain and source, this checkout's,
 * with the same tests to run, each ran its share, every test has one
 * result, and every known failure is a test. A run that isn't whole isn't
 * checked or blessed. */
export function validate(shards: Shard[], known: Map<string, string>, source: string): string[] {
  if (shards.length === 0) return ["there are no shards"];
  // What's read from a file is checked as it is, not as its type says.
  const strings = (value: unknown) => Array.isArray(value) && value.every((item) => typeof item === "string");
  const record = (value: unknown) => {
    if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
    const shard = value as Record<string, unknown>;
    return (
      Number.isInteger(shard.shard) &&
      Number.isInteger(shard.of) &&
      (shard.of as number) > 0 &&
      ["compiler", "toolchain", "source"].every((key) => typeof shard[key] === "string") &&
      strings(shard.inventory) &&
      strings(shard.expected) &&
      Array.isArray(shard.results)
    );
  };
  const malformed = shards.filter((shard) => !record(shard)).length;
  if (malformed > 0) return [`${malformed} of the ${shards.length} files aren't a shard's record`];
  const problems: string[] = [];
  // A result is a test's name and a status, and a reason for any but a pass.
  const result = (value: unknown) => {
    if (typeof value !== "object" || value === null) return false;
    const r = value as Record<string, unknown>;
    if (typeof r.test !== "string") return false;
    if (r.status === "pass") return true;
    return ["fail", "skip", "native"].includes(r.status as string) && typeof r.reason === "string" && r.reason !== "";
  };
  const invalid = shards.flatMap((shard) => shard.results).filter((r) => !result(r));
  if (invalid.length > 0) {
    const names = invalid.map((r) => (typeof (r as { test?: unknown })?.test === "string" ? (r as { test: string }).test : "?"));
    problems.push(`results that aren't one: ${invalid.length}, as ${names.slice(0, 3).join(", ")}`);
  }
  const [first] = shards;
  for (const key of ["of", "compiler", "toolchain", "source"] as const) {
    const values = new Set(shards.map((shard) => String(shard[key])));
    if (values.size > 1) problems.push(`the shards ran with different ${key}s: ${[...values].join(", ")}`);
  }
  if (first.source !== source) problems.push(`the shards ran source ${first.source}, and this is ${source}`);
  const inventory = first.inventory.join("\n");
  if (first.inventory.length === 0) problems.push("there were no tests to run");
  if (shards.some((shard) => shard.inventory.join("\n") !== inventory)) problems.push("the shards had different tests to run");
  const counts = Array.from({ length: first.of }, (_, k) => shards.filter((shard) => shard.shard === k + 1).length);
  const numbers = (n: (count: number) => boolean) => counts.flatMap((count, k) => (n(count) ? [k + 1] : []));
  const absent = numbers((count) => count === 0), repeated = numbers((count) => count > 1);
  if (absent.length > 0) problems.push(`shards missing, of ${first.of}: ${absent.slice(0, 10).join(", ")}${absent.length > 10 ? ", .." : ""}`);
  if (repeated.length > 0) problems.push(`shards there more than once, of ${first.of}: ${repeated.join(", ")}`);
  for (const shard of shards) {
    const share = first.inventory.filter((_, k) => k % first.of === shard.shard - 1);
    if (share.join("\n") !== shard.expected.join("\n")) problems.push(`shard ${shard.shard} was to run other tests than its share`);
  }
  const results = new Map<string, number>();
  for (const r of shards.flatMap((shard) => shard.results)) results.set(r.test, (results.get(r.test) ?? 0) + 1);
  const tests = new Set(first.inventory);
  const some = (list: string[]) => `${list.length}, as ${list.slice(0, 3).join(", ")}`;
  const missing = first.inventory.filter((test) => !results.has(test));
  const twice = [...results].filter(([, n]) => n > 1).map(([test]) => test);
  const extra = [...results.keys()].filter((test) => !tests.has(test));
  const stale = [...known.keys()].filter((test) => !tests.has(test));
  if (missing.length > 0) problems.push(`tests with no result: ${some(missing)}`);
  if (twice.length > 0) problems.push(`tests with more than one result: ${some(twice)}`);
  if (extra.length > 0) problems.push(`results of tests there weren't: ${some(extra)}`);
  if (stale.length > 0) problems.push(`known failures that aren't tests: ${some(stale)}`);
  return problems;
}

/** This checkout's commit. */
const source = () => Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim();

/** Did a process end otherwise than by exiting 0? */
const failed = (exit: Exit, timeout: number) => exit.code !== 0 || stopped(exit, timeout) !== undefined;

export async function runTest(ui: string, file: string): Promise<Result> {
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
    if (failed(build, 120_000)) return { test, status: "native", reason: `rustc: ${stopped(build, 120_000) ?? firstError(build.stderr)}` };
    const native = await run([binary], dirname(file), 10_000);
    if (failed(native, 10_000)) return { test, status: "native", reason: `doesn't pass natively with overflow checks off: ${stopped(native, 10_000) ?? `exited ${native.code}`}` };
    // What a `HashMap` prints, say, changes from run to run: no answer to compare with.
    for (let i = 0; i < 2; i++) {
      const again = await run([binary], dirname(file), 10_000);
      if (failed(again, 10_000)) return { test, status: "native", reason: "doesn't pass natively on every run" };
      if (again.stdout !== native.stdout || again.stderr !== native.stderr) {
        return { test, status: "native", reason: "prints what changes from run to run" };
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

export function findTests(ui: string): string[] {
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

/** `test<TAB>reason` lines: what rust-js gets wrong, and how, or what
 * native Rust gives no answer for, and why. */
function readKnown(file = knownFile): Map<string, string> {
  const known = new Map<string, string>();
  if (!existsSync(file)) return known;
  for (const line of readFileSync(file, "utf8").split("\n")) {
    if (line === "" || line.startsWith("#")) continue;
    const [test, reason = ""] = line.split("\t");
    known.set(test, reason);
  }
  return known;
}

/** The known failures, what native Rust gives no answer for, and what's
 * out of scope: every test that doesn't pass, and why. */
const lists = (): [Map<string, string>, Map<string, string>, Map<string, string>] => [
  readKnown(),
  readKnown(nativeFile),
  readKnown(outOfScopeFile),
];

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
    skipped: Object.fromEntries([...skipped].sort((a, b) => b[1] - a[1])),
    noNativeAnswer: count("native"),
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
  const listed = (status: string) => results.filter((r): r is Result & { reason: string } => r.status === status);
  if (bless) {
    const failing = listed("fail"), unanswered = listed("native"), skipped = listed("skip");
    const blessed = "# Rewritten by `bun scripts/rustc-suite.ts --bless`.\n";
    const lines = (list: { test: string; reason: string }[]) => list.map((r) => `${r.test}\t${r.reason}\n`).join("");
    writeFileSync(knownFile, `# rustc run-pass UI tests rust-js gets wrong, and its first error (ADR 0089).\n${blessed}${lines(failing)}`);
    writeFileSync(nativeFile, `# rustc run-pass UI tests native Rust gives no answer for here, and why (ADR 0089).\n${blessed}${lines(unanswered)}`);
    writeFileSync(outOfScopeFile, `# rustc run-pass UI tests out of scope, and why (ADR 0089).\n${blessed}${lines(skipped)}`);
    const wrote = `${failing.length} known failures, ${unanswered.length} tests native Rust gives no answer for, and ${skipped.length} out of scope`;
    console.log(`wrote ${wrote}`);
    toSummary([`Wrote ${wrote}.`]);
    return;
  }
  const { regressions, fixed, worse, unanswered, answered, excluded, included } = ratchet(results, ...lists());
  for (const r of regressions) console.log(`FAILS\t${r.test}\t${r.reason}`);
  for (const r of fixed) console.log(`PASSES\t${r.test}\tremove it from test/rustc-known-failures.txt`);
  for (const r of worse) console.log(`WORSE\t${r.test}\t${failureKind(r.reason)}, listed as rejected: ${r.reason}`);
  for (const r of unanswered) console.log(`NO ANSWER\t${r.test}\tnative Rust: ${r.reason}`);
  for (const r of answered) console.log(`ANSWERS\t${r.test}\tremove it from test/rustc-native-failures.txt`);
  for (const r of excluded) console.log(`OUT OF SCOPE\t${r.test}\t${r.reason}`);
  for (const r of included) console.log(`IN SCOPE\t${r.test}\tremove it from test/rustc-out-of-scope.txt`);
  const changes = regressions.length + fixed.length + worse.length + unanswered.length + answered.length + excluded.length + included.length;
  if (changes === 0) {
    toSummary(["The known failures are as listed."]);
    return;
  }
  toSummary([
    "| Test | Now | |",
    "|---|---|---|",
    ...regressions.map((r) => `| ${r.test} | fails | ${cell(r.reason)} |`),
    ...fixed.map((r) => `| ${r.test} | passes | take it off the known failures |`),
    ...worse.map((r) => `| ${r.test} | ${failureKind(r.reason)} | listed as rejected: ${cell(r.reason)} |`),
    ...unanswered.map((r) => `| ${r.test} | no native answer | ${cell(r.reason)} |`),
    ...answered.map((r) => `| ${r.test} | native Rust answers | take it off the native failures |`),
    ...excluded.map((r) => `| ${r.test} | out of scope | ${cell(r.reason)} |`),
    ...included.map((r) => `| ${r.test} | in scope | take it off the tests out of scope |`),
  ]);
  console.log(
    `${regressions.length} newly failing, ${fixed.length} newly passing, ${worse.length} failing worse, ${unanswered.length} newly without a native answer, ${answered.length} newly with one, ${excluded.length} newly out of scope, ${included.length} newly in: run with --bless once they're intended`,
  );
  process.exitCode = 1;
}

async function main() {
  const args = process.argv.slice(2);
  const bless = args.includes("--bless");
  const option = (name: string) => args.find((a) => a.startsWith(`--${name}=`))?.slice(name.length + 3);
  const out = option("out");
  const shard = option("shard");
  // `--merge a.json b.json`: the shards' results, as one run, if they
  // are one whole run.
  if (args.includes("--merge")) {
    const files = args.filter((a) => !a.startsWith("--"));
    const shards = files.map((f) => JSON.parse(readFileSync(f, "utf8")) as Shard);
    const problems = validate(shards, new Map(lists().flatMap((list) => [...list])), source());
    if (problems.length > 0) {
      for (const problem of problems) console.log(`INCOMPLETE\t${problem}`);
      toSummary(["## rustc run-pass tests", "", "The run isn't whole, so it isn't checked:", "", ...problems.map((p) => `- ${cell(p)}`)]);
      process.exitCode = 1;
      return;
    }
    report(shards.flatMap((shard) => shard.results).sort((a, b) => a.test.localeCompare(b.test)), bless);
    return;
  }
  const selectors = args.filter((a) => !a.startsWith("--"));
  compiler = resolve(option("compiler") ?? compiler);
  const ui = rustcTests();
  if (!existsSync(compiler)) throw new Error(`no rust-js at ${compiler}: build it with cargo build --release`);
  const all = selectors.length > 0 ? [...new Set(selectors.flatMap((s) => testsUnder(ui, s)))] : findTests(ui);
  let tests = all;
  // `--shard=2/4`: every fourth test, from the second.
  if (shard) {
    const [i, n] = shard.split("/").map(Number);
    if (!(n > 0 && i >= 1 && i <= n)) throw new Error(`--shard=${shard}: say which of how many, as 2/4`);
    if (selectors.length > 0) throw new Error("--shard runs every test: name none");
    tests = all.filter((_, k) => k % n === i - 1);
    const results = await runAll(ui, tests);
    const record: Shard = {
      shard: i,
      of: n,
      compiler: new Bun.CryptoHasher("sha256").update(readFileSync(compiler)).digest("hex"),
      toolchain: rustcCommit(),
      source: source(),
      inventory: all.map((file) => relative(ui, file)),
      expected: tests.map((file) => relative(ui, file)),
      results,
    };
    if (out) writeFileSync(out, JSON.stringify(record));
    console.log(JSON.stringify(summarize(results), null, 2));
    return;
  }
  const results = await runAll(ui, tests);
  if (selectors.length > 0) {
    // Some tests: each, and whether it's what the known failures say.
    const unlisted = surprises(results, ...lists());
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
  // A whole run is one shard of one, checked as the workflow's are.
  const names = all.map((file) => relative(ui, file));
  const whole: Shard = { shard: 1, of: 1, compiler: "", toolchain: "", source: source(), inventory: names, expected: names, results };
  const problems = validate([whole], new Map(lists().flatMap((list) => [...list])), source());
  for (const problem of problems) console.log(`INCOMPLETE\t${problem}`);
  if (problems.length > 0) {
    process.exitCode = 1;
    return;
  }
  // The whole run's results, which a run of some tests leaves as they were.
  writeFileSync(join(work, "results.json"), JSON.stringify(results, null, 2));
  report(results, bless);
}

if (import.meta.main) await main();
