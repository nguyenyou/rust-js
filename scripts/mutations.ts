// Mutations of the compiler, each a bug it had, or one a rule of it keeps
// out, put back: the tests named for each must fail against a compiler
// built with it, and pass against the compiler as it is (ADR 0093). A
// mutation that no longer applies, doesn't build, or that its tests don't
// catch fails the run, so the tests are shown to see what they're for.
//
//   bun scripts/mutations.ts                  # every mutation
//   bun scripts/mutations.ts copy-on-read ..  # the ones named
//
// The mutations themselves are in `mutations/`, one list for each source
// file. Each builds natively a few test programs, which macOS makes slow:
// run them in the Linux VM (AGENTS.md).

import { chmodSync, cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { runSync, stopped, type Exit } from "../test/child";

const root = join(import.meta.dir, "..");

export type Mutation = {
  name: string;
  /** What it breaks, as Rust would see it. */
  breaks: string;
  file: string;
  find: string;
  replace: string;
  /** `bun test` arguments whose tests must catch it. */
  tests: string[];
  /** Its tests check the corpus's JS snapshots too: it changes only how
   * the JS reads, as one that takes away the readable form of something
   * a more general one also does right, and how it reads is what's checked. */
  snapshots?: boolean;
};

// The mutations, one list for each source file, by its path from `src/`:
// `src/lower/traits.rs`'s are in `mutations/lower/traits.ts`.
export const lists: { path: string; mutations: Mutation[] }[] = [];
for (const path of [...new Bun.Glob("**/*.ts").scanSync(join(import.meta.dir, "mutations"))].sort()) {
  lists.push({ path, mutations: (await import(`./mutations/${path}`)).mutations });
}
export const mutations: Mutation[] = lists.flatMap((list) => list.mutations);

// Where the mutated crate is built, and the compilers kept: one copy of
// the crate, remade for each mutation, and one target, so only rust-js is
// built again.
const work = join(root, "target", "mutants");
const crate = join(work, "crate");
const crateFiles = ["Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "src"];
const buildTimeout = 20 * 60_000;
const testTimeout = 20 * 60_000;

/** The source as it is, with `mutation` in it, or why it can't be. */
export function mutate(source: string, mutation: Mutation): string | { problem: string } {
  const at = source.indexOf(mutation.find);
  if (at < 0) return { problem: `doesn't apply: ${mutation.file} has no \`${mutation.find.split("\n")[0].trim()}\`` };
  if (source.indexOf(mutation.find, at + 1) >= 0) return { problem: `applies more than once in ${mutation.file}` };
  return source.slice(0, at) + mutation.replace + source.slice(at + mutation.find.length);
}

/** A compiler built from this checkout's crate, with `mutation` in it, or
 * without one; or why it can't be built. */
function build(mutation?: Mutation): string | { problem: string } {
  rmSync(crate, { recursive: true, force: true });
  mkdirSync(crate, { recursive: true });
  for (const file of crateFiles) cpSync(join(root, file), join(crate, file), { recursive: true });
  if (mutation) {
    const file = join(crate, mutation.file);
    const mutated = mutate(readFileSync(file, "utf8"), mutation);
    if (typeof mutated !== "string") return mutated;
    writeFileSync(file, mutated);
  }
  const target = join(work, "target");
  const p = runSync(["cargo", "build", "--quiet", "--locked", "--target-dir", target], crate, buildTimeout);
  if (p.code !== 0 || stopped(p, buildTimeout)) {
    const why = stopped(p, buildTimeout) ?? p.stderr.split("\n").find((line) => line.startsWith("error")) ?? `exited ${p.code}`;
    return { problem: `doesn't build: ${why}` };
  }
  const kept = join(work, "bin", mutation?.name ?? "unmutated");
  mkdirSync(join(work, "bin"), { recursive: true });
  cpSync(join(target, "debug", "rust-js"), kept);
  return kept;
}

/** `run`, with the checkout's `@rust-js/runtime` the one `compiler` writes:
 * the JS imports its helpers from the package (ADR 0103), so a helper's
 * mutation is the package's too. The checkout's is put back after. One
 * that writes none, as the control that compiles nothing, runs with the
 * checkout's. */
function withRuntime<T>(compiler: string, run: () => T): T {
  const file = join(root, "runtime", "index.js");
  const original = readFileSync(file, "utf8");
  const written = runSync([compiler, "--runtime-module"], root, buildTimeout);
  if (written.code !== 0 || stopped(written, buildTimeout)) return run();
  writeFileSync(file, written.stdout);
  try {
    return run();
  } finally {
    writeFileSync(file, original);
  }
}

const count = (output: string, what: string) => Number(new RegExp(String.raw`^ (\d+) ` + what + "$", "m").exec(output)?.[1] ?? 0);

/** What a run of a mutant's tests says of it: `caught` by a test that
 * failed, the runner ending as it does when one does; `survived`, as its
 * tests ran and passed; or `inconclusive`, as the runner, its tests or
 * their hooks ran out of time, it was stopped, or failed before any test
 * did, which says nothing of it. */
export function judge(p: Exit, output: string): "caught" | "survived" | "inconclusive" {
  if (stopped(p, testTimeout)) return "inconclusive";
  if (p.code === 0) return count(output, "pass") > 0 && count(output, "fail") === 0 ? "survived" : "inconclusive";
  // Not only at a line's start: a long diff bun test cuts short doesn't end
  // its last line, and the `(fail)` after it is printed on that line.
  const failed = [...output.matchAll(/\(fail\) (.*)$\n?(  \^ .* timed out\b)?/gm)].filter(
    (m) => !m[1].startsWith("(unnamed)") && m[2] === undefined,
  );
  return p.code === 1 && failed.length > 0 ? "caught" : "inconclusive";
}

/** How `tests` do with `compiler`, what they printed, and how many ran. */
function test(tests: string[], compiler: string, snapshots = false): { passed: boolean; ran: number; output: string; exit: Exit } {
  // What the JS does is what's checked: a corpus snapshot differs with
  // nearly any change to the compiler, a mutation's or not. One that only
  // changes how the JS reads is checked by its snapshots.
  const p = withRuntime(compiler, () =>
    runSync([process.execPath, "test", ...tests], root, testTimeout, {
      RUST_JS_COMPILER: compiler,
      // A compile that takes long, as an exponential one, is stopped, and
      // its test fails, before the runner runs out of time on it, which
      // would say nothing: a corpus case compiles in a second or two.
      RUST_JS_COMPILE_TIMEOUT: "60000",
      ...(snapshots ? {} : { RUST_JS_SNAPSHOTS: "ignore" }),
    }),
  );
  const output = p.stdout + p.stderr;
  return { passed: p.code === 0 && !stopped(p, testTimeout), ran: count(output, "pass") + count(output, "fail"), output, exit: p };
}

async function main() {
  const named = process.argv.slice(2);
  const unknown = named.filter((name) => !mutations.some((m) => m.name === name));
  if (unknown.length > 0) throw new Error(`no mutation ${unknown.join(", ")}; there are ${mutations.map((m) => m.name).join(", ")}`);
  const chosen = named.length > 0 ? mutations.filter((m) => named.includes(m.name)) : mutations;
  // Each mutation's tests pass as the compiler is, and run at all, so
  // their failing is the mutation's doing.
  const unmutated = build();
  if (typeof unmutated !== "string") throw new Error(`the compiler as it is ${unmutated.problem}`);
  // And they use the compiler they're given: with one that compiles
  // nothing, each fails, or a mutation passing them would say nothing.
  const broken = join(work, "bin", "broken");
  writeFileSync(broken, "#!/bin/sh\necho 'error: rust-js compiles nothing here' >&2\nexit 101\n");
  chmodSync(broken, 0o755);
  // Each set of tests as its mutations run them, with snapshots or without.
  for (const key of new Set(chosen.map((m) => [m.snapshots ? "snapshots" : "", ...m.tests].join("\0")))) {
    const [mode, ...tests] = key.split("\0");
    const control = test(tests, unmutated, mode === "snapshots");
    if (!control.passed || control.ran === 0) {
      throw new Error(`\`bun test ${tests.join(" ")}\` doesn't pass, or runs nothing, as the compiler is:\n${control.output.slice(-2000)}`);
    }
    if (test(tests, broken, mode === "snapshots").passed) {
      throw new Error(`\`bun test ${tests.join(" ")}\` passes with a compiler that compiles nothing: it isn't using the one it's given`);
    }
  }
  const rows: [Mutation, string][] = [];
  for (const mutation of chosen) {
    const compiler = build(mutation);
    if (typeof compiler !== "string") {
      rows.push([mutation, compiler.problem]);
      continue;
    }
    const { ran, output, exit } = test(mutation.tests, compiler, mutation.snapshots);
    // Its log, whatever it says, for what it caught or didn't.
    const log = join(work, "logs", `${mutation.name}.log`);
    mkdirSync(join(work, "logs"), { recursive: true });
    writeFileSync(log, output);
    const verdict = judge(exit, output);
    rows.push([
      mutation,
      verdict === "caught" ? "caught" : verdict === "survived" ? `SURVIVED: its ${ran} tests passed with it` : `INCONCLUSIVE: no test failed, or the runner didn't end; see ${log}`,
    ]);
  }
  for (const [mutation, result] of rows) console.log(`${mutation.name}\t${result}`);
  const summary = process.env.GITHUB_STEP_SUMMARY;
  if (summary) {
    const cell = (s: string) => s.replaceAll("|", "\\|");
    const lines = ["## Mutations", "", "| Mutation | Breaks | Result |", "|---|---|---|"];
    for (const [mutation, result] of rows) lines.push(`| ${mutation.name} | ${cell(mutation.breaks)} | ${cell(result)} |`);
    writeFileSync(summary, lines.join("\n") + "\n", { flag: "a" });
  }
  const missed = rows.filter(([, result]) => result !== "caught");
  console.log(`${rows.length - missed.length} of ${rows.length} mutations caught`);
  if (missed.length > 0) process.exitCode = 1;
}

if (import.meta.main) await main();
