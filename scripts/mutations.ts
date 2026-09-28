// Mutations of the compiler, each a bug it had, or one a rule of it keeps
// out, put back: the tests named for each must fail against a compiler
// built with it, and pass against the compiler as it is (ADR 0093). A
// mutation that no longer applies, doesn't build, or that its tests don't
// catch fails the run, so the tests are shown to see what they're for.
//
//   bun scripts/mutations.ts                  # every mutation
//   bun scripts/mutations.ts copy-on-read ..  # the ones named
//
// Each builds natively a few test programs, which macOS makes slow; the
// rustc tests workflow runs them all on Linux with `mutations`.

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
};

export const mutations: Mutation[] = [
  {
    name: "element-value-first",
    breaks: "`v[i] = f()` checks `i` before `f` runs, and reads what the index wrote",
    file: "src/lower.rs",
    find: "        if (writes && !value.is_constant()) || (panics && value.has_effects()) {",
    replace: "        if false && ((writes && !value.is_constant()) || (panics && value.has_effects())) {",
    tests: ["test/corpus.test.ts", "-t", "assignment_order"],
  },
  {
    name: "compound-place-read",
    breaks: "`x += g()` reads `x` before `g`, which changes it, runs",
    file: "src/lower.rs",
    find: "let rhs_js = if rhs_js.has_effects() && self.may_change(lhs) {",
    replace: "let rhs_js = if false && rhs_js.has_effects() && self.may_change(lhs) {",
    tests: ["test/corpus.test.ts", "-t", "assignment_order\\.rs"],
  },
  {
    name: "i32-wrap",
    breaks: "`i32` arithmetic doesn't wrap at 32 bits",
    file: "src/lower/representation.rs",
    find: "            Num::I32 => Expr::bin(Op::BitOr, e, Expr::num(0)),",
    replace: "            Num::I32 => e,",
    tests: ["test/corpus.test.ts", "-t", "wrapping\\.rs"],
  },
  {
    name: "u64-wrap",
    breaks: "`u64` arithmetic doesn't wrap at 64 bits",
    file: "src/lower/representation.rs",
    find: '            Num::U64 => as_n("asUintN"),',
    replace: "            Num::U64 => e,",
    tests: ["test/corpus.test.ts", "-t", "wrapping\\.rs"],
  },
  {
    name: "index-panic-message",
    breaks: "an index out of bounds panics with another message than Rust's",
    file: "src/runtime.rs",
    find: String.raw`function $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(` + "`index out of bounds: the len is",
    replace: String.raw`function $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(` + "`index out of bounds: the length is",
    tests: ["test/corpus.test.ts", "-t", "index_out_of_bounds"],
  },
  {
    name: "copy-on-read",
    breaks: "a `Copy` value read from a place is that place, not a copy",
    file: "src/lower/representation.rs",
    find: "        if self.contains_mutated(ty) && self.is_copy(ty) {",
    replace: "        if false && self.contains_mutated(ty) && self.is_copy(ty) {",
    tests: ["test/corpus.test.ts", "-t", "copy_mutation"],
  },
  {
    name: "guard-statements",
    breaks: "a guard's statements don't run before its test",
    file: "src/lower.rs",
    find: "                before.push(StmtKind::If(guard, body, None).at(span));",
    replace: "                before.clear();\n                before.push(StmtKind::If(guard, body, None).at(span));",
    tests: ["test/corpus.test.ts", "-t", "guard_statements"],
  },
  {
    name: "crash-after-rejection",
    breaks: "rust-js panics after it says what it doesn't support",
    file: "src/lower.rs",
    find: '        self.tcx\n            .dcx()\n            .span_err(span, format!("rust-js does not support {what} yet"))\n    }',
    replace: '        self.tcx.dcx().span_err(span, format!("rust-js does not support {what} yet"));\n        panic!("a crash after the rejection")\n    }',
    tests: ["test/corpus.test.ts", "-t", "union_const|closure_clone\\.rs"],
  },
  {
    name: "operand-capture",
    breaks: "an earlier operand runs after a later one's statements",
    file: "src/lower.rs",
    find: "            if !evaluated.statements.is_empty() {",
    replace: "            if false {",
    tests: ["test/semantics.test.ts", "-t", "operand_prerequisites"],
  },
  {
    name: "union-field",
    breaks: "a union's field is read as a struct's, and rust-js panics",
    file: "src/lower.rs",
    find: "    ty.ty_adt_def().is_some_and(|adt| adt.is_union())",
    replace: "    ty.ty_adt_def().is_some_and(|adt| adt.is_union() && false)",
    tests: ["test/corpus.test.ts", "-t", "union_const"],
  },
  {
    name: "dyn-bound-lifetimes",
    breaks: "a `dyn for<'a>` trait's dictionary is asked of rustc with its lifetime bound, and rustc panics",
    file: "src/lower/traits.rs",
    find: "                self.tcx\n                    .instantiate_bound_regions_with_erased(p.with_self_ty(self.tcx, self_ty))",
    replace: "                p.with_self_ty(self.tcx, self_ty).skip_binder()",
    tests: ["test/corpus.test.ts", "-t", "higher_ranked_dyn"],
  },
  {
    name: "begin-panic-payload",
    breaks: "`panic!(5)` before edition 2021 throws a message Rust never shows",
    file: "src/lower/calls.rs",
    find: "                if !text {",
    replace: "                if false && !text {",
    tests: ["test/corpus.test.ts", "-t", "begin_panic_value"],
  },
  {
    name: "lazy-rhs-statements",
    breaks: "`a || f(&mut y)` runs the statements `f`'s call needs whether or not `a` decides",
    file: "src/lower.rs",
    find: "                    if rhs_out.is_empty() {\n                        return Ok(Expr::bin(js_op, l, r));",
    replace: "                    if true {\n                        out.extend(rhs_out);\n                        return Ok(Expr::bin(js_op, l, r));",
    tests: ["test/corpus.test.ts", "-t", "lazy_effects"],
  },
  {
    name: "while-condition-statements",
    breaks: "a `while` condition's statements are put in the loop, after its test",
    file: "src/lower.rs",
    find: "                if before.is_empty() {\n                    self.stmt(then, &Dest::Discard, &mut body_out)?;",
    replace: "                if true {\n                    body_out.extend(before);\n                    self.stmt(then, &Dest::Discard, &mut body_out)?;",
    tests: ["test/corpus.test.ts", "-t", "lazy_effects"],
  },
  {
    name: "at-binding-copy",
    breaks: "a binding after `@` reads its part of the value in place, which the binding before it changes",
    file: "src/lower.rs",
    find: "        let stable = stable && !(bindings.len() > 1 && bindings.iter().any(|b| b.whole));",
    replace: "        let stable = stable || bindings.iter().any(|b| b.whole);",
    tests: ["test/corpus.test.ts", "-t", "binding_after_at"],
  },
  {
    name: "option-some-rest",
    breaks: "`Some(..)`, whose `..` names no field, is taken as `None`",
    file: "src/lower.rs",
    find: "                    let op = if some { Op::LooseNe } else { Op::LooseEq };",
    replace: "                    let op = if some && false { Op::LooseNe } else { Op::LooseEq };",
    tests: ["test/corpus.test.ts", "-t", "option_rest_pattern"],
  },
  {
    name: "size-align-swap",
    breaks: "`align_of` is the type's size",
    file: "src/lower/calls.rs",
    find: "                let bytes = if matches!(known, Std::AlignOf) {\n                    layout.align.abi.bytes()",
    replace: "                let bytes = if matches!(known, Std::AlignOf) {\n                    layout.size.bytes()",
    tests: ["test/corpus.test.ts", "-t", "size_of\\.rs"],
  },
  {
    name: "array-repeat-shared",
    breaks: "`[x; N]` of what's changed is one object, `N` times",
    file: "src/lower.rs",
    find: "                let copied = if self.is_copy(item_ty) {\n                    self.contains_mutated(item_ty)",
    replace: "                let copied = if self.is_copy(item_ty) {\n                    false",
    tests: ["test/corpus.test.ts", "-t", "array_repeat"],
  },
  {
    name: "never-loop-value",
    breaks: "a `loop` that never ends, used as a value, is rejected",
    file: "src/lower.rs",
    find: "            ExprKind::NeverToAny { source } => match self.thir[source].kind {\n                ExprKind::Loop { body } => Some(body),",
    replace: "            ExprKind::NeverToAny { source } => match self.thir[source].kind {\n                ExprKind::Loop { body } if false => Some(body),",
    tests: ["test/corpus.test.ts", "-t", "loop_values"],
  },
];

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

const count = (output: string, what: string) => Number(new RegExp(String.raw`^ (\d+) ` + what + "$", "m").exec(output)?.[1] ?? 0);

/** What a run of a mutant's tests says of it: `caught` by a test that
 * failed, the runner ending as it does when one does; `survived`, as its
 * tests ran and passed; or `inconclusive`, as the runner ran out of time,
 * was stopped, or failed before any test did, which says nothing of it. */
export function judge(p: Exit, output: string): "caught" | "survived" | "inconclusive" {
  if (stopped(p, testTimeout)) return "inconclusive";
  if (p.code === 0) return count(output, "pass") > 0 && count(output, "fail") === 0 ? "survived" : "inconclusive";
  const failed = [...output.matchAll(/^\(fail\) (.*)$/gm)].filter((m) => !m[1].startsWith("(unnamed)"));
  return p.code === 1 && failed.length > 0 ? "caught" : "inconclusive";
}

/** How `tests` do with `compiler`, what they printed, and how many ran. */
function test(tests: string[], compiler: string): { passed: boolean; ran: number; output: string; exit: Exit } {
  const p = runSync([process.execPath, "test", ...tests], root, testTimeout, { RUST_JS_COMPILER: compiler });
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
  for (const tests of new Set(chosen.map((m) => m.tests.join("\0")))) {
    const control = test(tests.split("\0"), unmutated);
    if (!control.passed || control.ran === 0) {
      throw new Error(`\`bun test ${tests.split("\0").join(" ")}\` doesn't pass, or runs nothing, as the compiler is:\n${control.output.slice(-2000)}`);
    }
    if (test(tests.split("\0"), broken).passed) {
      throw new Error(`\`bun test ${tests.split("\0").join(" ")}\` passes with a compiler that compiles nothing: it isn't using the one it's given`);
    }
  }
  const rows: [Mutation, string][] = [];
  for (const mutation of chosen) {
    const compiler = build(mutation);
    if (typeof compiler !== "string") {
      rows.push([mutation, compiler.problem]);
      continue;
    }
    const { ran, output, exit } = test(mutation.tests, compiler);
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
