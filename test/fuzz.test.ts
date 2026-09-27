// Generated programs (ADR 0092): each seed's program run natively and as JS,
// as the corpus's are, which must print and end the same. One that doesn't
// is reduced to as few lines as still differ, and written with its seed to
// target/fuzz/, to become a corpus case once it's fixed.
//
//   bun test test/fuzz.test.ts                                  # the first 12 seeds, which must compile
//   FUZZ_START=1000 FUZZ_SEEDS=500 bun test test/fuzz.test.ts   # 500 more

import { beforeAll, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { generate, print, reduce, size, type Program } from "./generate";
import { agree, compileJs, runJs, runNative, runtimes, show, type Run } from "./programs";
import { buildCompiler, fixture, target } from "./support";

const start = Number(process.env.FUZZ_START ?? 1);
const seeds = Number(process.env.FUZZ_SEEDS ?? 12);
// The seeds `bun test` runs are known to compile, so one rust-js rejects is
// a regression; exploring others, a program it doesn't support is skipped.
const exploring = process.env.FUZZ_START !== undefined || process.env.FUZZ_SEEDS !== undefined;

beforeAll(buildCompiler);

type Verdict =
  | { kind: "same" }
  | { kind: "unsupported"; error: string }
  | { kind: "invalid"; error: string }
  | { kind: "crash"; error: string }
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
    // else, an error it can't explain or its own panic, isn't.
    return compiled.error.includes("rust-js does not support")
      ? { kind: "unsupported", error: compiled.error }
      : { kind: "crash", error: compiled.error };
  }
  const js = runtimes.map(([name, cmd]): [string, Run] => [name, runJs(cmd, compiled.js, dir, name)]);
  return js.every(([, run]) => agree(run, native)) ? { kind: "same" } : { kind: "differs", native, js };
}

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
    // As small as it still fails the same way.
    const smallest = await reduce(program, async (candidate) => judge(candidate, seed).kind === verdict.kind);
    const out = join(target, "fuzz");
    mkdirSync(out, { recursive: true });
    const repro = join(out, `seed-${seed}.rs`);
    writeFileSync(repro, print(smallest, seed));
    const again = judge(smallest, seed);
    const detail =
      again.kind === "differs"
        ? [`native: ${show(again.native.outcome)}\n${again.native.stdout}`, ...again.js.map(([name, run]) => `${name}: ${show(run.outcome)}\n${run.stdout}${run.stderr}`)].join("\n")
        : "error" in again ? again.error : "";
    expect(`${verdict.kind}, reduced from ${size(program)} statements to ${size(smallest)} in ${repro}:\n\n${print(smallest, seed)}\n${detail}`).toBe("");
  }, 600_000);
}
