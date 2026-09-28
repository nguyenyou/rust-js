// Generated programs (ADR 0092): each seed's program run natively and as JS,
// as the corpus's are, which must print and end the same. One that doesn't
// is written to target/fuzz/ at once, with its seed, the compiler and what
// each run printed, then reduced to as few lines as still fail the same way,
// each smaller one written as it's found, to become a corpus case once it's
// fixed.
//
//   bun test test/fuzz.test.ts                                  # the first 12 seeds, which must compile
//   FUZZ_START=1000 FUZZ_SEEDS=500 bun test test/fuzz.test.ts   # 500 more

import { beforeAll, expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { generate, print, reduce, size, type Program } from "./generate";
import { same } from "./oracle";
import { agree, compileJs, runJs, runNative, runtimes, show, type Run } from "./programs";
import { buildCompiler, compiler, fixture, root, target } from "./support";

const start = Number(process.env.FUZZ_START ?? 1);
const seeds = Number(process.env.FUZZ_SEEDS ?? 12);
// The seeds `bun test` runs are known to compile, so one rust-js rejects is
// a regression; exploring others, a program it doesn't support is skipped.
const exploring = process.env.FUZZ_START !== undefined || process.env.FUZZ_SEEDS !== undefined;

// A reduction stops after this long, with the smallest program so far.
const reduceBudget = Number(process.env.FUZZ_REDUCE_BUDGET ?? 300_000);

beforeAll(buildCompiler);

type Verdict =
  | { kind: "same" }
  | { kind: "unsupported"; error: string }
  | { kind: "invalid"; error: string }
  | { kind: "crash"; error: string; reason: string }
  | { kind: "differs"; native: Run; js: [string, Run][] };

/** What happens to a program, natively and as JS. */
function judge(program: Program, seed: number): Verdict {
  const dir = fixture(`fuzz-${seed}`);
  const file = join(dir, "program.rs");
  writeFileSync(file, print(program, seed));
  const native = runNative(file, dir);
  if (typeof native === "string") return { kind: "invalid", error: native };
  const compiled = compileJs(file, dir);
  if ("error" in compiled) {
    // A feature rust-js says it doesn't support is its answer; anything
    // else, an error it can't explain or its own panic, isn't, even after it.
    return compiled.kind === "rejected"
      ? { kind: "unsupported", error: compiled.error }
      : { kind: "crash", error: compiled.error, reason: compiled.reason };
  }
  const js = runtimes.map(([name, cmd]): [string, Run] => [name, runJs(cmd, compiled.js, dir, name)]);
  return js.every(([, run]) => agree(run, native)) ? { kind: "same" } : { kind: "differs", native, js };
}

/** How a program fails, which a smaller one must fail the same way: which
 * runtimes differ from native Rust in which of stdout, stderr and how it
 * ended, or what crashed. */
function signature(verdict: Verdict): string {
  if (verdict.kind === "crash") return `crash: ${verdict.reason}`;
  if (verdict.kind !== "differs") return verdict.kind;
  const { native } = verdict;
  return JSON.stringify(
    verdict.js.map(([name, run]) => [
      name,
      run.stdout === native.stdout,
      run.stderr === native.stderr,
      typeof run.outcome !== "string" && typeof native.outcome !== "string" && same(run.outcome, native.outcome),
    ]),
  );
}

/** What each run printed and how it ended, or what the compiler said. */
function detail(verdict: Verdict): string {
  if (verdict.kind === "differs") {
    const { native, js } = verdict;
    return [`native: ${show(native.outcome)}\n${native.stdout}`, ...js.map(([name, run]) => `${name}: ${show(run.outcome)}\n${run.stdout}${run.stderr}`)].join("\n");
  }
  return "error" in verdict ? verdict.error : "";
}

// Which compiler and source a failure came from, to run it again.
let provenance: { compiler: string; sha256: string; source: string } | undefined;
const origin = () =>
  (provenance ??= {
    compiler,
    sha256: new Bun.CryptoHasher("sha256").update(readFileSync(compiler)).digest("hex"),
    source: Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim(),
  });

for (let seed = start; seed < start + seeds; seed++) {
  test(`seed ${seed}`, async () => {
    const program = generate(seed);
    const verdict = judge(program, seed);
    if (process.env.FUZZ_REPORT) console.log(`seed ${seed}: ${verdict.kind}${"error" in verdict ? ` ${verdict.error.split("\n")[0]}` : ""}`);
    // A generated program is valid Rust, or the generator is wrong.
    if (verdict.kind === "invalid") throw new Error(`seed ${seed} made a program rustc rejects:\n${verdict.error}`);
    if (verdict.kind === "unsupported" && !exploring) {
      throw new Error(`seed ${seed} compiled before, and rust-js rejects it now:\n${verdict.error}`);
    }
    if (verdict.kind === "same" || verdict.kind === "unsupported") return;
    // What failed, kept before anything else is tried, so a reduction that
    // never ends, or a job stopped, can't lose it.
    const out = join(target, "fuzz");
    mkdirSync(out, { recursive: true });
    const failure = signature(verdict);
    writeFileSync(join(out, `seed-${seed}.original.rs`), print(program, seed));
    const evidence = { seed, ...origin(), kind: verdict.kind, signature: failure, detail: detail(verdict) };
    writeFileSync(join(out, `seed-${seed}.json`), JSON.stringify(evidence, null, 2));
    // As small as it still fails the same way, each smaller one kept.
    const repro = join(out, `seed-${seed}.rs`);
    const keep = (p: Program) => writeFileSync(repro, print(p, seed));
    keep(program);
    const { program: smallest, complete } = await reduce(program, async (candidate) => signature(judge(candidate, seed)) === failure, {
      until: Date.now() + reduceBudget,
      kept: keep,
    });
    const again = judge(smallest, seed);
    const reduced = complete ? "reduced" : `reduced for ${reduceBudget / 1000}s, not to the end,`;
    expect(`${verdict.kind}, ${reduced} from ${size(program)} statements to ${size(smallest)} in ${repro}:\n\n${print(smallest, seed)}\n${detail(again)}`).toBe("");
  }, 600_000);
}
