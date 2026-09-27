// Runs rustc's own `run-pass` UI tests, at the pinned toolchain's commit, as
// corpus cases (ADR 0089): each is built natively and with rust-js, and the
// JS must print what the native binary prints, under Bun and Node.
//
//   bun scripts/rustc-suite.ts            # check against the known failures
//   bun scripts/rustc-suite.ts --bless    # rewrite the known failures
//   bun scripts/rustc-suite.ts path/to/test.rs ..   # run some, and say why
//
// A test rust-js gets wrong is listed, with its first error, in
// test/rustc-known-failures.txt. One that isn't listed must pass, and one
// that is must still fail: when it passes, it's taken off, so the list only
// shrinks.

import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { availableParallelism, homedir } from "node:os";
import { basename, dirname, join, relative } from "node:path";

import { rustcTests } from "./rustc-tests";

const root = join(import.meta.dir, "..");
const compiler = join(root, "target", "debug", "rust-js");
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

/** What's changed since the known failures were written: a test that fails
 * and isn't listed, and one that's listed and passes. */
export function ratchet(results: Result[], known: Map<string, string>) {
  return {
    regressions: results.filter((r): r is Result & { reason: string } => r.status === "fail" && !known.has(r.test)),
    fixed: results.filter((r) => r.status === "pass" && known.has(r.test)),
  };
}

async function spawn(cmd: string[], cwd: string, timeout: number) {
  const p = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "pipe", timeout });
  const [stdout, stderr] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  return { stdout, stderr, code: p.exitCode, killed: p.signalCode !== null };
}

/** A diagnostic's first line, as short as it can say it, with no path
 * that's this machine's: the list is the same wherever it's written. */
export const firstError = (stderr: string) =>
  (stderr.split("\n").find((line) => line.startsWith("error")) ?? stderr.trim().split("\n")[0] ?? "no output")
    .replaceAll(/\/[^\s`:]*\/rustc-suite\/case-[^/\s`:]*/g, "<case>")
    .replaceAll(homedir(), "~")
    .slice(0, 200);

async function runTest(ui: string, file: string): Promise<Result> {
  const test = relative(ui, file);
  const source = readFileSync(file, "utf8");
  const s = scope(source);
  if ("skip" in s) return { test, status: "skip", reason: s.skip };
  const dir = mkdtempSync(join(work, "case-"));
  try {
    // Natively, as rust-js takes Rust: the release profile, where arithmetic wraps.
    const binary = join(dir, "native");
    const build = await spawn(
      ["rustc", `--edition=${s.edition}`, "-Coverflow-checks=off", "-Awarnings", file, "-o", binary],
      dirname(file),
      120_000,
    );
    if (build.code !== 0) return { test, status: "skip", reason: `rustc: ${firstError(build.stderr)}` };
    const native = await spawn([binary], dirname(file), 10_000);
    if (native.killed || native.code !== 0) return { test, status: "skip", reason: "doesn't pass natively with overflow checks off" };
    // What a `HashMap` prints, say, changes from run to run: no answer to compare with.
    for (let i = 0; i < 2; i++) {
      const again = await spawn([binary], dirname(file), 10_000);
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
    const compiled = await spawn([compiler, lib, "-o", js, "--", `--edition=${s.edition}`, "-Awarnings"], dir, 120_000);
    if (compiled.code !== 0) return { test, status: "fail", reason: firstError(compiled.stderr) };
    for (const [name, runtime] of [["bun", process.execPath], ["node", "node"]]) {
      const outcomeFile = join(dir, `${name}.json`);
      const run = await spawn([runtime, join(root, "test", "corpus-run.ts"), js, outcomeFile], dir, 10_000);
      if (run.killed) return { test, status: "fail", reason: `${name}: didn't end in 10s` };
      const outcome = existsSync(outcomeFile) ? readFileSync(outcomeFile, "utf8") : `exited ${run.code}: ${firstError(run.stderr)}`;
      if (outcome !== '{"value":null}') return { test, status: "fail", reason: `${name}: ended ${outcome.slice(0, 200)}` };
      if (run.stdout !== native.stdout) return { test, status: "fail", reason: `${name}: different stdout` };
      if (run.stderr !== native.stderr) return { test, status: "fail", reason: `${name}: different stderr` };
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

async function main() {
  const args = process.argv.slice(2);
  const bless = args.includes("--bless");
  const only = args.filter((a) => !a.startsWith("--"));
  const ui = rustcTests();
  if (!existsSync(compiler)) throw new Error("build rust-js first: cargo build");
  mkdirSync(work, { recursive: true });
  const tests = only.length > 0 ? only.map((t) => join(ui, t)) : findTests(ui);

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
  results.sort((a, b) => a.test.localeCompare(b.test));

  const count = (status: string) => results.filter((r) => r.status === status).length;
  const skipped = new Map<string, number>();
  for (const r of results) if (r.status === "skip") skipped.set(r.reason, (skipped.get(r.reason) ?? 0) + 1);
  const summary = {
    commit: dirname(dirname(ui)).split("/").pop(),
    tests: results.length,
    inScope: count("pass") + count("fail"),
    pass: count("pass"),
    fail: count("fail"),
    skipped: Object.fromEntries([...skipped].sort((a, b) => b[1] - a[1]).filter(([reason]) => !reason.startsWith("rustc: "))),
    skippedByRustc: [...skipped].filter(([reason]) => reason.startsWith("rustc: ")).reduce((n, [, k]) => n + k, 0),
  };
  console.log(JSON.stringify(summary, null, 2));

  if (only.length > 0) {
    for (const r of results) console.log(`${r.status}\t${r.test}${"reason" in r ? `\t${r.reason}` : ""}`);
    return;
  }
  // The whole run's results, which a run of some tests leaves as they were.
  writeFileSync(join(work, "results.json"), JSON.stringify(results, null, 2));
  const failing = results.filter((r): r is Result & { reason: string } => r.status === "fail");
  if (bless) {
    const header = "# rustc run-pass UI tests rust-js gets wrong, and its first error (ADR 0089).\n# Rewritten by `bun scripts/rustc-suite.ts --bless`.\n";
    writeFileSync(knownFile, header + failing.map((r) => `${r.test}\t${r.reason}\n`).join(""));
    console.log(`wrote ${failing.length} known failures`);
    return;
  }
  const { regressions, fixed } = ratchet(results, readKnown());
  for (const r of regressions) console.log(`FAILS\t${r.test}\t${r.reason}`);
  for (const r of fixed) console.log(`PASSES\t${r.test}\tremove it from test/rustc-known-failures.txt`);
  if (regressions.length > 0 || fixed.length > 0) {
    console.log(`${regressions.length} newly failing, ${fixed.length} newly passing: run with --bless once they're intended`);
    process.exit(1);
  }
}

if (import.meta.main) await main();
